//! Virtual schema filesystem and libxml2 resolvers.
//!
//! Schema dependencies (`xs:include` / `xs:import`) are discovered and loaded
//! asynchronously via the JavaScript loader **before** `xmlSchemaParse` runs.
//! JavaScript is never called from a libxml2 C callback.
//!
//! Resolution during `xmlSchemaParse` uses two layers:
//! 1. Per-parse `xmlSchemaSetResourceLoader` (preferred, carries its own VFS).
//! 2. Process-wide `xmlRegisterInputCallbacks` backed by a **thread-local** VFS
//!    stack — required because libxml2 creates nested schema parser contexts
//!    for included documents and does not copy `resourceLoader` onto them, so
//!    nested includes fall through to the default I/O path.
//!
//! Concurrent `XsdValidator.create()` calls are safe: each compile thread pushes
//! its own VFS onto the thread-local stack for the duration of `xmlSchemaParse`.

use crate::error::{napi_error, ErrorCollector};
use crate::libxml::{
  cstr_to_string, xmlBuildURI, xmlNewInputFromMemory, xmlParserErrors, xmlParserInput,
  xmlRegisterInputCallbacks, xmlResourceType, xml_char_to_string, xsd_collect_schema_locations,
};
use napi::threadsafe_function::ThreadsafeFunction;
use napi::{bindgen_prelude::Either, bindgen_prelude::Promise, Result, Status};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::ffi::CString;
use std::os::raw::{c_char, c_int, c_void};
use std::path::{Component, Path};
use std::ptr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Once;

/// In-memory map of virtual schema path → XSD source text.
pub type VirtualSchemaFs = HashMap<String, String>;

/// JS loader: `(filename: string) => string | Promise<string>`
pub type SchemaLoader =
  ThreadsafeFunction<String, Either<String, Promise<String>>, String, Status, false>;

const ROOT_SCHEMA_PATH: &str = "root.xsd";

static INPUT_CALLBACKS_INIT: Once = Once::new();
static INPUT_CALLBACKS_REGISTERED: AtomicBool = AtomicBool::new(false);

thread_local! {
  /// Stack of VFS pointers for the current thread's in-flight schema compiles.
  static ACTIVE_VFS: RefCell<Vec<*const VirtualSchemaFs>> = const { RefCell::new(Vec::new()) };
}

/// Owns the virtual filesystem used while compiling a schema.
pub struct SchemaStore {
  pub vfs: VirtualSchemaFs,
  pub root_path: String,
}

/// Recursively resolve `xs:include` / `xs:import` dependencies by calling the
/// JavaScript loader on the Tokio/napi async context (never from libxml2 C).
///
/// When `loader` is `None`, the root schema must not reference other files.
pub async fn build_schema_store(
  root_xsd: String,
  loader: Option<&SchemaLoader>,
) -> Result<SchemaStore> {
  let root_path = ROOT_SCHEMA_PATH.to_string();
  let mut vfs: VirtualSchemaFs = HashMap::new();
  vfs.insert(root_path.clone(), root_xsd);

  let mut seen: HashSet<String> = HashSet::new();
  seen.insert(root_path.clone());

  let mut queue: VecDeque<String> = VecDeque::new();
  queue.push_back(root_path.clone());

  while let Some(current_path) = queue.pop_front() {
    let content = vfs
      .get(&current_path)
      .cloned()
      .ok_or_else(|| napi_error(format!("schema '{}' missing from VFS", current_path)))?;

    let refs = extract_schema_locations(&content, &current_path)?;
    for location in refs {
      let resolved = resolve_schema_location(&current_path, &location);
      if !seen.insert(resolved.clone()) {
        continue;
      }

      let Some(loader) = loader else {
        return Err(napi_error(format!(
          "schema references '{}' but no loader was provided",
          resolved
        )));
      };

      let loaded = call_loader(loader, &resolved).await?;
      if loaded.trim().is_empty() {
        return Err(napi_error(format!(
          "schema loader returned empty content for '{}'",
          resolved
        )));
      }

      vfs.insert(resolved.clone(), loaded);
      queue.push_back(resolved);
    }
  }

  Ok(SchemaStore { vfs, root_path })
}

async fn call_loader(loader: &SchemaLoader, filename: &str) -> Result<String> {
  let result = loader
    .call_async_catch(filename.to_string())
    .await
    .map_err(|err| napi_error(format!("schema loader failed for '{}': {}", filename, err)))?;

  match result {
    Either::A(s) => Ok(s),
    Either::B(promise) => promise.await.map_err(|err| {
      napi_error(format!(
        "schema loader promise rejected for '{}': {}",
        filename, err
      ))
    }),
  }
}

/// Parse an XSD document and return every `schemaLocation` from
/// `xs:include` / `xs:import` (and `xs:redefine` / `xs:override`).
pub fn extract_schema_locations(xsd: &str, base_path: &str) -> Result<Vec<String>> {
  let mut collector = ErrorCollector::new();

  let c_url = CString::new(base_path)
    .map_err(|_| napi_error(format!("schema path contains interior nul: {}", base_path)))?;

  let mut locations: Vec<String> = Vec::new();
  let rc = unsafe {
    xsd_collect_schema_locations(
      xsd.as_ptr() as *const c_char,
      xsd.len() as c_int,
      c_url.as_ptr(),
      Some(schema_ref_callback),
      &mut locations as *mut Vec<String> as *mut c_void,
    )
  };

  if rc != 0 {
    unsafe {
      if let Some(err) = crate::libxml::xmlGetLastError().as_ref() {
        collector.push_error(err);
      }
      crate::libxml::xmlResetLastError();
    }
    return Err(collector.into_napi_error(&format!("failed to parse schema '{}'", base_path)));
  }

  Ok(locations)
}

unsafe extern "C" fn schema_ref_callback(ctx: *mut c_void, location: *const c_char) {
  if ctx.is_null() || location.is_null() {
    return;
  }
  let out = &mut *(ctx as *mut Vec<String>);
  if let Some(value) = cstr_to_string(location) {
    let trimmed = value.trim();
    if !trimmed.is_empty() {
      out.push(trimmed.to_string());
    }
  }
}

/// Resolve `location` against the virtual path of the referencing schema.
pub fn resolve_schema_location(base: &str, location: &str) -> String {
  let location = location.trim();
  if location.is_empty() {
    return base.to_string();
  }

  if is_absolute_uri(location) {
    return location.to_string();
  }

  if let Ok(base_c) = CString::new(base) {
    if let Ok(loc_c) = CString::new(location) {
      unsafe {
        let built = xmlBuildURI(loc_c.as_ptr() as *const u8, base_c.as_ptr() as *const u8);
        if !built.is_null() {
          let resolved = xml_char_to_string(built).unwrap_or_else(|| location.to_string());
          crate::libxml::xml_free_xmlchar(built);
          return normalize_vfs_key(&resolved);
        }
      }
    }
  }

  normalize_vfs_key(&join_relative(base, location))
}

fn is_absolute_uri(value: &str) -> bool {
  if value.starts_with('/') || value.starts_with('\\') {
    return true;
  }
  if let Some(idx) = value.find(':') {
    let scheme = &value[..idx];
    return !scheme.is_empty()
      && scheme
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '-' || c == '.');
  }
  false
}

fn join_relative(base: &str, location: &str) -> String {
  let parent = match base.rfind('/') {
    Some(idx) => &base[..=idx],
    None => "",
  };
  format!("{}{}", parent, location)
}

fn normalize_vfs_key(path: &str) -> String {
  if is_absolute_uri(path) && path.contains("://") {
    return path.to_string();
  }

  let mut parts: Vec<String> = Vec::new();
  for component in Path::new(path).components() {
    match component {
      Component::CurDir => {}
      Component::ParentDir => {
        parts.pop();
      }
      Component::Normal(seg) => {
        parts.push(seg.to_string_lossy().into_owned());
      }
      Component::RootDir => {
        parts.clear();
      }
      Component::Prefix(_) => {}
    }
  }

  if path.starts_with('/') {
    format!("/{}", parts.join("/"))
  } else {
    parts.join("/")
  }
}

/// Look up a URL in the VFS using exact and lightly normalized keys.
pub fn vfs_lookup<'a>(vfs: &'a VirtualSchemaFs, url: &str) -> Option<&'a String> {
  if let Some(content) = vfs.get(url) {
    return Some(content);
  }

  let normalized = normalize_vfs_key(url);
  if let Some(content) = vfs.get(&normalized) {
    return Some(content);
  }

  let trimmed = url.trim_start_matches("./");
  if trimmed != url {
    if let Some(content) = vfs.get(trimmed) {
      return Some(content);
    }
  }

  // Windows may surface backslash paths from libxml2 WINPATH handling.
  let forward = url.replace('\\', "/");
  if forward != url {
    if let Some(content) = vfs.get(&forward) {
      return Some(content);
    }
    let normalized = normalize_vfs_key(&forward);
    if let Some(content) = vfs.get(&normalized) {
      return Some(content);
    }
  }

  if let Some(name) = Path::new(url).file_name() {
    let name = name.to_string_lossy();
    let matches: Vec<_> = vfs
      .iter()
      .filter(|(k, _)| {
        Path::new(k)
          .file_name()
          .map(|f| f.to_string_lossy() == name)
          .unwrap_or(false)
      })
      .collect();
    if matches.len() == 1 {
      return Some(matches[0].1);
    }
  }

  None
}

fn current_vfs() -> Option<*const VirtualSchemaFs> {
  ACTIVE_VFS.with(|stack| stack.borrow().last().copied())
}

/// libxml2 `xmlResourceLoader` backed by [`VirtualSchemaFs`].
pub unsafe extern "C" fn vfs_resource_loader(
  ctxt: *mut c_void,
  url: *const c_char,
  _public_id: *const c_char,
  _type: xmlResourceType,
  _flags: c_int,
  out: *mut *mut xmlParserInput,
) -> xmlParserErrors {
  if ctxt.is_null() || url.is_null() || out.is_null() {
    return xmlParserErrors::XML_ERR_INTERNAL_ERROR;
  }

  *out = ptr::null_mut();

  let vfs = &*(ctxt as *const VirtualSchemaFs);
  let url_str = match cstr_to_string(url) {
    Some(s) => s,
    None => return xmlParserErrors::XML_IO_ENOENT,
  };

  let Some(content) = vfs_lookup(vfs, &url_str) else {
    return xmlParserErrors::XML_IO_ENOENT;
  };

  // XML_INPUT_BUF_STATIC requires a NUL byte at content[len], which a Rust
  // String does not guarantee, so let libxml2 copy the bytes instead.
  let input = xmlNewInputFromMemory(url, content.as_ptr() as *const c_void, content.len(), 0);

  if input.is_null() {
    return xmlParserErrors::XML_ERR_NO_MEMORY;
  }

  *out = input;
  xmlParserErrors::XML_ERR_OK
}

/// RAII guard that publishes the VFS to this thread for classic input callbacks.
///
/// Nested schema parser contexts created by libxml2 do not inherit
/// `resourceLoader`, so includes/imports resolved from those contexts fall
/// through to the process-wide input callbacks, which read this thread-local.
pub struct InputCallbackGuard;

impl InputCallbackGuard {
  pub fn install(vfs: &VirtualSchemaFs) -> Result<Self> {
    ensure_input_callbacks_registered()?;
    ACTIVE_VFS.with(|stack| {
      stack.borrow_mut().push(vfs as *const VirtualSchemaFs);
    });
    Ok(Self)
  }
}

impl Drop for InputCallbackGuard {
  fn drop(&mut self) {
    ACTIVE_VFS.with(|stack| {
      stack.borrow_mut().pop();
    });
  }
}

fn ensure_input_callbacks_registered() -> Result<()> {
  let mut error: Option<String> = None;
  INPUT_CALLBACKS_INIT.call_once(|| {
    let rc = unsafe {
      xmlRegisterInputCallbacks(
        Some(vfs_input_match),
        Some(vfs_input_open),
        Some(vfs_input_read),
        Some(vfs_input_close),
      )
    };
    if rc < 0 {
      error = Some("xmlRegisterInputCallbacks failed".to_string());
    } else {
      INPUT_CALLBACKS_REGISTERED.store(true, Ordering::SeqCst);
    }
  });

  if let Some(message) = error {
    return Err(napi_error(message));
  }
  if !INPUT_CALLBACKS_REGISTERED.load(Ordering::SeqCst) {
    return Err(napi_error("libxml2 input callbacks are not registered"));
  }
  Ok(())
}

struct VfsInputContext {
  data: Vec<u8>,
  offset: usize,
}

unsafe extern "C" fn vfs_input_match(filename: *const c_char) -> c_int {
  if filename.is_null() {
    return 0;
  }
  let url = match cstr_to_string(filename) {
    Some(s) => s,
    None => return 0,
  };

  let Some(ptr) = current_vfs() else {
    return 0;
  };
  let vfs = &*ptr;
  if vfs_lookup(vfs, &url).is_some() {
    1
  } else {
    0
  }
}

unsafe extern "C" fn vfs_input_open(filename: *const c_char) -> *mut c_void {
  if filename.is_null() {
    return ptr::null_mut();
  }
  let url = match cstr_to_string(filename) {
    Some(s) => s,
    None => return ptr::null_mut(),
  };

  let Some(ptr) = current_vfs() else {
    return ptr::null_mut();
  };
  let vfs = &*ptr;
  let Some(content) = vfs_lookup(vfs, &url) else {
    return ptr::null_mut();
  };

  let ctx = Box::new(VfsInputContext {
    data: content.as_bytes().to_vec(),
    offset: 0,
  });
  Box::into_raw(ctx) as *mut c_void
}

unsafe extern "C" fn vfs_input_read(
  context: *mut c_void,
  buffer: *mut c_char,
  len: c_int,
) -> c_int {
  if context.is_null() || buffer.is_null() || len <= 0 {
    return -1;
  }
  let ctx = &mut *(context as *mut VfsInputContext);
  if ctx.offset >= ctx.data.len() {
    return 0;
  }
  let remaining = ctx.data.len() - ctx.offset;
  let to_copy = remaining.min(len as usize);
  ptr::copy_nonoverlapping(
    ctx.data.as_ptr().add(ctx.offset),
    buffer as *mut u8,
    to_copy,
  );
  ctx.offset += to_copy;
  to_copy as c_int
}

unsafe extern "C" fn vfs_input_close(context: *mut c_void) -> c_int {
  if context.is_null() {
    return -1;
  }
  drop(Box::from_raw(context as *mut VfsInputContext));
  0
}

/// Expose root path constant for schema compilation.
pub fn root_schema_path() -> &'static str {
  ROOT_SCHEMA_PATH
}
