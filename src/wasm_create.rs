//! `XsdValidator.create` for threadless wasm targets.
//!
//! napi-rs rejects every built-in `async fn` on `wasm32-wasip1` because there
//! is no thread to run Tokio on. Schema loading is instead driven as a chain of
//! JavaScript promises on the single JS thread: call the loader, continue in
//! `.then` when it returns a promise, and compile once every reference has
//! been supplied.

use crate::error::napi_error;
use crate::resolver::{missing_loader_error, SchemaResolution};
use crate::{compile_schema, release_parser, retain_parser};
use napi::bindgen_prelude::*;
use napi::{sys, JsValue};

type Loader<'env> = Function<'env, String, Unknown<'static>>;
type LoaderRef = FunctionRef<String, Unknown<'static>>;

pub fn create<'env>(
  env: &'env Env,
  root_xsd: String,
  loader: Option<Loader<'env>>,
) -> Result<PromiseRaw<'env, Unknown<'static>>> {
  let loader = loader.map(|f| f.create_ref()).transpose()?;

  // Every step runs inside `.then`, so validation and loader errors reject the
  // returned promise instead of throwing synchronously.
  PromiseRaw::<()>::resolve(env, ())?.then(move |ctx| {
    if root_xsd.trim().is_empty() {
      return Err(napi_error("root XSD must not be empty"));
    }

    Box::new(PendingCreate {
      resolution: SchemaResolution::new(root_xsd),
      loader,
      _parser: ParserRef::retain(),
    })
    .advance(&ctx.env)
  })
}

struct PendingCreate {
  resolution: SchemaResolution,
  loader: Option<LoaderRef>,
  _parser: ParserRef,
}

impl PendingCreate {
  fn advance(mut self: Box<Self>, env: &Env) -> Result<Unknown<'static>> {
    while let Some(path) = self.resolution.next_request()? {
      let Some(loader) = &self.loader else {
        return Err(missing_loader_error(&path));
      };

      let result = loader
        .borrow_back(env)?
        .call(path.clone())
        .map_err(|err| napi_error(format!("schema loader failed for '{}': {}", path, err)))?;

      match result.get_type()? {
        ValueType::String => {
          let content = unsafe { String::from_napi_value(env.raw(), result.raw()) }?;
          self.resolution.supply(path, content)?;
        }
        _ if is_promise(env, &result)? => {
          let promise = unsafe { PromiseRaw::<String>::from_napi_value(env.raw(), result.raw()) }?;
          let next = promise.then(move |ctx| {
            self.resolution.supply(path, ctx.value)?;
            self.advance(&ctx.env)
          })?;
          return Ok(unsafe { Unknown::from_raw_unchecked(env.raw(), next.raw()) });
        }
        _ => {
          return Err(napi_error(format!(
            "schema loader must return a string or a Promise<string> for '{}'",
            path
          )));
        }
      }
    }

    let PendingCreate {
      resolution,
      _parser: parser,
      ..
    } = *self;
    let validator = compile_schema(resolution.finish())?;
    // The validator now owns the parser reference and releases it on drop.
    std::mem::forget(parser);

    let value = unsafe { ToNapiValue::to_napi_value(env.raw(), validator) }?;
    Ok(unsafe { Unknown::from_raw_unchecked(env.raw(), value) })
  }
}

fn is_promise(env: &Env, value: &Unknown) -> Result<bool> {
  let mut result = false;
  napi::check_status!(unsafe { sys::napi_is_promise(env.raw(), value.raw(), &mut result) })?;
  Ok(result)
}

/// Holds one libxml2 parser reference while a create is in flight, so a
/// rejected loader promise or a compile error releases it.
struct ParserRef;

impl ParserRef {
  fn retain() -> Self {
    retain_parser();
    Self
  }
}

impl Drop for ParserRef {
  fn drop(&mut self) {
    release_parser();
  }
}
