//! napi-rs bindings for XSD validation via vendored libxml2.

#![deny(clippy::all)]

mod error;
mod libxml;
mod resolver;

use error::{
  clear_thread_collector, napi_error, set_thread_collector, structured_error_callback,
  ErrorCollector,
};
use libxml::{
  xmlCleanupParser, xmlFreeDoc, xmlInitParser, xmlReadMemory, xmlResetLastError,
  xmlSchemaCleanupTypes, xmlSchemaFree, xmlSchemaFreeParserCtxt, xmlSchemaFreeValidCtxt,
  xmlSchemaNewParserCtxt, xmlSchemaNewValidCtxt, xmlSchemaParse,
  xmlSchemaSetParserStructuredErrors, xmlSchemaSetResourceLoader,
  xmlSchemaSetValidStructuredErrors, xmlSchemaValidateDoc, XML_PARSE_COMPACT, XML_PARSE_NONET,
};
use napi::bindgen_prelude::*;
use napi_derive::napi;
#[cfg(not(target_family = "wasm"))]
use resolver::{build_schema_store, SchemaLoader};
use resolver::{root_schema_path, vfs_resource_loader, InputCallbackGuard, SchemaStore};

#[cfg(target_family = "wasm")]
mod wasm_create;
use std::ffi::CString;
use std::os::raw::{c_char, c_void};
use std::ptr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Once;

static PARSER_INIT: Once = Once::new();
static PARSER_REFS: AtomicUsize = AtomicUsize::new(0);

fn retain_parser() {
  PARSER_INIT.call_once(|| unsafe {
    xmlInitParser();
  });
  PARSER_REFS.fetch_add(1, Ordering::SeqCst);
}

fn release_parser() {
  let _ = PARSER_REFS.fetch_sub(1, Ordering::SeqCst);
}

/// Process-level teardown for libxml2 globals.
///
/// Prefer letting the Node process exit normally. Only call this when you are
/// certain no `XsdValidator` instances remain alive.
#[napi]
pub fn shutdown_libxml() {
  unsafe {
    xmlSchemaCleanupTypes();
    xmlCleanupParser();
  }
}

/// Compiles an XSD (with recursively resolved includes/imports) and validates XML
/// documents against the resulting schema.
#[napi]
pub struct XsdValidator {
  schema: *mut libxml::xmlSchema,
  /// Keep VFS alive for the lifetime of the compiled schema as insurance
  /// against libxml2 retaining any pointers into schema source text.
  #[allow(dead_code)]
  store: SchemaStore,
}

unsafe impl Send for XsdValidator {}
unsafe impl Sync for XsdValidator {}

#[napi]
impl XsdValidator {
  /// Create a validator from a root XSD string and an optional schema loader.
  ///
  /// The loader is invoked only during this call, on the Node/Tokio side, to
  /// fetch `xs:include` / `xs:import` targets. It is never invoked from a
  /// libxml2 C callback thread. Omit the loader when the root XSD has no
  /// external references.
  #[cfg(not(target_family = "wasm"))]
  #[napi(ts_args_type = "rootXsd: string, loader?: (filename: string) => string | Promise<string>")]
  pub async fn create(root_xsd: String, loader: Option<SchemaLoader>) -> Result<Self> {
    if root_xsd.trim().is_empty() {
      return Err(napi_error("root XSD must not be empty"));
    }

    retain_parser();

    let store = match build_schema_store(root_xsd, loader.as_ref()).await {
      Ok(store) => store,
      Err(err) => {
        release_parser();
        return Err(err);
      }
    };

    match compile_schema(store) {
      Ok(validator) => Ok(validator),
      Err(err) => {
        release_parser();
        Err(err)
      }
    }
  }

  /// Same JS contract as the native `create`, whose declaration `index.d.ts`
  /// carries: the impl-level `#[napi]` sees both cfg variants and would emit
  /// a second one.
  #[cfg(target_family = "wasm")]
  #[napi(skip_typescript)]
  pub fn create<'env>(
    env: &'env Env,
    root_xsd: String,
    loader: Option<Function<'env, String, Unknown<'static>>>,
  ) -> Result<PromiseRaw<'env, Unknown<'static>>> {
    wasm_create::create(env, root_xsd, loader)
  }

  /// Validate an XML document string against the compiled schema.
  ///
  /// Returns `true` when the document is well-formed and schema-valid.
  /// Returns `false` when the document is invalid. Throws only on internal
  /// failures (e.g. unable to allocate a validation context).
  #[napi]
  pub fn validate(&self, xml: String) -> Result<bool> {
    if self.schema.is_null() {
      return Err(napi_error("validator schema is not initialized"));
    }

    let mut collector = ErrorCollector::new();
    let collector_ptr = &mut collector as *mut ErrorCollector;

    unsafe {
      set_thread_collector(collector_ptr);
      xmlResetLastError();

      let doc = xmlReadMemory(
        xml.as_ptr() as *const c_char,
        xml.len() as i32,
        ptr::null(),
        ptr::null(),
        XML_PARSE_NONET | XML_PARSE_COMPACT,
      );

      if doc.is_null() {
        if let Some(err) = libxml::xmlGetLastError().as_ref() {
          collector.push_error(err);
        }
        xmlResetLastError();
        clear_thread_collector();
        // Malformed XML → not valid.
        return Ok(false);
      }

      let vctxt = xmlSchemaNewValidCtxt(self.schema);
      if vctxt.is_null() {
        xmlFreeDoc(doc);
        clear_thread_collector();
        return Err(napi_error("failed to create XML Schema validation context"));
      }

      xmlSchemaSetValidStructuredErrors(
        vctxt,
        Some(structured_error_callback),
        collector_ptr as *mut c_void,
      );

      let code = xmlSchemaValidateDoc(vctxt, doc);

      xmlSchemaFreeValidCtxt(vctxt);
      xmlFreeDoc(doc);
      xmlResetLastError();
      clear_thread_collector();

      // libxml2: 0 = valid, positive = validation error, -1 = internal error
      if code < 0 {
        return Err(collector.into_napi_error("internal error while validating XML against XSD"));
      }

      Ok(code == 0)
    }
  }

  /// Validate and, on failure, throw a JavaScript exception containing libxml2
  /// diagnostics. On success returns `true`.
  #[napi]
  pub fn validate_or_throw(&self, xml: String) -> Result<bool> {
    if self.schema.is_null() {
      return Err(napi_error("validator schema is not initialized"));
    }

    let mut collector = ErrorCollector::new();
    let collector_ptr = &mut collector as *mut ErrorCollector;

    unsafe {
      set_thread_collector(collector_ptr);
      xmlResetLastError();

      let doc = xmlReadMemory(
        xml.as_ptr() as *const c_char,
        xml.len() as i32,
        ptr::null(),
        ptr::null(),
        XML_PARSE_NONET | XML_PARSE_COMPACT,
      );

      if doc.is_null() {
        if let Some(err) = libxml::xmlGetLastError().as_ref() {
          collector.push_error(err);
        }
        xmlResetLastError();
        clear_thread_collector();
        return Err(collector.into_napi_error("XML document is not well-formed"));
      }

      let vctxt = xmlSchemaNewValidCtxt(self.schema);
      if vctxt.is_null() {
        xmlFreeDoc(doc);
        clear_thread_collector();
        return Err(napi_error("failed to create XML Schema validation context"));
      }

      xmlSchemaSetValidStructuredErrors(
        vctxt,
        Some(structured_error_callback),
        collector_ptr as *mut c_void,
      );

      let code = xmlSchemaValidateDoc(vctxt, doc);

      xmlSchemaFreeValidCtxt(vctxt);
      xmlFreeDoc(doc);
      xmlResetLastError();
      clear_thread_collector();

      if code == 0 {
        Ok(true)
      } else {
        Err(collector.into_napi_error("XML failed XSD validation"))
      }
    }
  }
}

impl Drop for XsdValidator {
  fn drop(&mut self) {
    unsafe {
      if !self.schema.is_null() {
        xmlSchemaFree(self.schema);
        self.schema = ptr::null_mut();
      }
    }
    release_parser();
  }
}

fn compile_schema(store: SchemaStore) -> Result<XsdValidator> {
  let mut collector = ErrorCollector::new();
  let collector_ptr = &mut collector as *mut ErrorCollector;

  let root_url = CString::new(store.root_path.as_str())
    .map_err(|_| napi_error("root schema path contains interior nul bytes"))?;

  // Thread-local input-callback fallback: nested schema parser contexts created
  // by libxml2 do not inherit resourceLoader, so includes from included documents
  // fall through to the process-wide I/O path.
  let _input_guard = InputCallbackGuard::install(&store.vfs)?;

  unsafe {
    set_thread_collector(collector_ptr);
    xmlResetLastError();

    let pctxt = xmlSchemaNewParserCtxt(root_url.as_ptr());
    if pctxt.is_null() {
      clear_thread_collector();
      return Err(napi_error("failed to create XML Schema parser context"));
    }

    xmlSchemaSetParserStructuredErrors(
      pctxt,
      Some(structured_error_callback),
      collector_ptr as *mut c_void,
    );
    xmlSchemaSetResourceLoader(
      pctxt,
      Some(vfs_resource_loader),
      &store.vfs as *const _ as *mut c_void,
    );

    let schema = xmlSchemaParse(pctxt);
    xmlSchemaFreeParserCtxt(pctxt);
    xmlResetLastError();
    clear_thread_collector();

    if schema.is_null() {
      return Err(collector.into_napi_error(&format!(
        "failed to compile XML Schema '{}'",
        root_schema_path()
      )));
    }

    Ok(XsdValidator { schema, store })
  }
}
