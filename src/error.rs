//! Error collection and conversion helpers for libxml2 → JavaScript.

use crate::libxml::{cstr_to_string, xmlError, xmlErrorLevel};
use napi::{Error, Status};
use std::cell::RefCell;
use std::ffi::c_void;
use std::os::raw::c_char;
use std::ptr;

/// Accumulates structured libxml2 errors for later conversion into a JS exception.
#[derive(Default, Debug, Clone)]
pub struct ErrorCollector {
  messages: Vec<String>,
}

impl ErrorCollector {
  pub fn new() -> Self {
    Self {
      messages: Vec::new(),
    }
  }

  pub fn push_raw(&mut self, message: String) {
    let trimmed = message.trim();
    if !trimmed.is_empty() {
      self.messages.push(trimmed.to_string());
    }
  }

  pub fn push_error(&mut self, error: &xmlError) {
    if error.level == xmlErrorLevel::XML_ERR_NONE {
      return;
    }

    let mut parts = Vec::new();

    unsafe {
      if let Some(file) = cstr_to_string(error.file) {
        if !file.is_empty() {
          if error.line > 0 {
            parts.push(format!("{}:{}", file, error.line));
          } else {
            parts.push(file);
          }
        }
      } else if error.line > 0 {
        parts.push(format!("line {}", error.line));
      }

      if let Some(message) = cstr_to_string(error.message) {
        let msg = message.trim();
        if !msg.is_empty() {
          parts.push(msg.to_string());
        }
      }
    }

    if parts.is_empty() {
      parts.push(format!("libxml2 error code {}", error.code));
    }

    self.messages.push(parts.join(": "));
  }

  pub fn into_message(self, fallback: &str) -> String {
    if self.messages.is_empty() {
      fallback.to_string()
    } else {
      self.messages.join("\n")
    }
  }

  pub fn into_napi_error(self, fallback: &str) -> Error {
    Error::new(Status::GenericFailure, self.into_message(fallback))
  }
}

thread_local! {
  static CURRENT_COLLECTOR: RefCell<*mut ErrorCollector> = const { RefCell::new(ptr::null_mut()) };
}

/// Install `collector` as the active error sink for the current thread.
///
/// # Safety
/// `collector` must remain valid until [`clear_thread_collector`] is called.
pub unsafe fn set_thread_collector(collector: *mut ErrorCollector) {
  CURRENT_COLLECTOR.with(|slot| {
    *slot.borrow_mut() = collector;
  });
}

pub fn clear_thread_collector() {
  CURRENT_COLLECTOR.with(|slot| {
    *slot.borrow_mut() = ptr::null_mut();
  });
}

/// libxml2 structured error callback that appends to the thread-local collector.
pub unsafe extern "C" fn structured_error_callback(user_data: *mut c_void, error: *const xmlError) {
  if error.is_null() {
    return;
  }

  let collector = if !user_data.is_null() {
    &mut *(user_data as *mut ErrorCollector)
  } else {
    let mut ptr = ptr::null_mut();
    CURRENT_COLLECTOR.with(|slot| {
      ptr = *slot.borrow();
    });
    if ptr.is_null() {
      return;
    }
    &mut *ptr
  };

  collector.push_error(&*error);
}

/// Format a simple owned C string message into a napi error.
pub fn napi_error(message: impl Into<String>) -> Error {
  Error::new(Status::GenericFailure, message.into())
}

/// Helper for callbacks that only receive a printf-style message pointer.
#[allow(dead_code)]
pub unsafe fn push_c_message(collector: *mut ErrorCollector, msg: *const c_char) {
  if collector.is_null() {
    return;
  }
  if let Some(message) = cstr_to_string(msg) {
    (*collector).push_raw(message);
  }
}
