//! Minimal FFI bindings for the libxml2 APIs used by this crate.

#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(dead_code)]

use std::os::raw::{c_char, c_int, c_void};

pub type xmlChar = u8;

#[repr(C)]
pub struct xmlDoc {
  _private: [u8; 0],
}

#[repr(C)]
pub struct xmlSchema {
  _private: [u8; 0],
}

#[repr(C)]
pub struct xmlSchemaParserCtxt {
  _private: [u8; 0],
}

#[repr(C)]
pub struct xmlSchemaValidCtxt {
  _private: [u8; 0],
}

#[repr(C)]
pub struct xmlParserInput {
  _private: [u8; 0],
}

#[repr(C)]
pub struct xmlError {
  pub domain: c_int,
  pub code: c_int,
  pub message: *mut c_char,
  pub level: xmlErrorLevel,
  pub file: *mut c_char,
  pub line: c_int,
  pub str1: *mut c_char,
  pub str2: *mut c_char,
  pub str3: *mut c_char,
  pub int1: c_int,
  pub int2: c_int,
  pub ctxt: *mut c_void,
  pub node: *mut c_void,
}

#[repr(i32)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum xmlElementType {
  XML_ELEMENT_NODE = 1,
  XML_ATTRIBUTE_NODE = 2,
  XML_TEXT_NODE = 3,
  XML_CDATA_SECTION_NODE = 4,
  XML_ENTITY_REF_NODE = 5,
  XML_ENTITY_NODE = 6,
  XML_PI_NODE = 7,
  XML_COMMENT_NODE = 8,
  XML_DOCUMENT_NODE = 9,
  XML_DOCUMENT_TYPE_NODE = 10,
  XML_DOCUMENT_FRAG_NODE = 11,
  XML_NOTATION_NODE = 12,
  XML_HTML_DOCUMENT_NODE = 13,
  XML_DTD_NODE = 14,
  XML_ELEMENT_DECL = 15,
  XML_ATTRIBUTE_DECL = 16,
  XML_ENTITY_DECL = 17,
  XML_NAMESPACE_DECL = 18,
  XML_XINCLUDE_START = 19,
  XML_XINCLUDE_END = 20,
}

#[repr(i32)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum xmlErrorLevel {
  XML_ERR_NONE = 0,
  XML_ERR_WARNING = 1,
  XML_ERR_ERROR = 2,
  XML_ERR_FATAL = 3,
}

#[repr(i32)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum xmlParserErrors {
  XML_ERR_OK = 0,
  XML_ERR_INTERNAL_ERROR = 1,
  XML_ERR_NO_MEMORY = 2,
  XML_IO_ENOENT = 1524,
}

#[repr(i32)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum xmlResourceType {
  XML_RESOURCE_UNKNOWN = 0,
  XML_RESOURCE_MAIN_DOCUMENT = 1,
  XML_RESOURCE_DTD = 2,
  XML_RESOURCE_GENERAL_ENTITY = 3,
  XML_RESOURCE_PARAMETER_ENTITY = 4,
  XML_RESOURCE_XINCLUDE = 5,
  XML_RESOURCE_XINCLUDE_TEXT = 6,
}

pub type xmlParserInputFlags = c_int;

pub type xmlParserOption = c_int;

pub const XML_PARSE_NONET: xmlParserOption = 1 << 11;
pub const XML_PARSE_COMPACT: xmlParserOption = 1 << 16;

/// Resource loader callback used by `xmlSchemaSetResourceLoader`.
pub type xmlResourceLoader = Option<
  unsafe extern "C" fn(
    ctxt: *mut c_void,
    url: *const c_char,
    public_id: *const c_char,
    type_: xmlResourceType,
    flags: xmlParserInputFlags,
    out: *mut *mut xmlParserInput,
  ) -> xmlParserErrors,
>;

pub type xmlStructuredErrorFunc =
  Option<unsafe extern "C" fn(user_data: *mut c_void, error: *const xmlError)>;

pub type xmlInputMatchCallback =
  Option<unsafe extern "C" fn(filename: *const c_char) -> c_int>;
pub type xmlInputOpenCallback =
  Option<unsafe extern "C" fn(filename: *const c_char) -> *mut c_void>;
pub type xmlInputReadCallback = Option<
  unsafe extern "C" fn(context: *mut c_void, buffer: *mut c_char, len: c_int) -> c_int,
>;
pub type xmlInputCloseCallback = Option<unsafe extern "C" fn(context: *mut c_void) -> c_int>;

extern "C" {
  // parser.h
  pub fn xmlInitParser();
  pub fn xmlCleanupParser();
  pub fn xmlReadMemory(
    buffer: *const c_char,
    size: c_int,
    url: *const c_char,
    encoding: *const c_char,
    options: c_int,
  ) -> *mut xmlDoc;
  pub fn xmlNewInputFromMemory(
    url: *const c_char,
    mem: *const c_void,
    size: usize,
    flags: xmlParserInputFlags,
  ) -> *mut xmlParserInput;

  // tree.h
  pub fn xmlFreeDoc(cur: *mut xmlDoc);

  // xmlschemas.h
  pub fn xmlSchemaNewParserCtxt(url: *const c_char) -> *mut xmlSchemaParserCtxt;
  pub fn xmlSchemaFreeParserCtxt(ctxt: *mut xmlSchemaParserCtxt);
  pub fn xmlSchemaSetParserStructuredErrors(
    ctxt: *mut xmlSchemaParserCtxt,
    serror: xmlStructuredErrorFunc,
    ctx: *mut c_void,
  );
  pub fn xmlSchemaSetResourceLoader(
    ctxt: *mut xmlSchemaParserCtxt,
    loader: xmlResourceLoader,
    data: *mut c_void,
  );
  pub fn xmlSchemaParse(ctxt: *mut xmlSchemaParserCtxt) -> *mut xmlSchema;
  pub fn xmlSchemaFree(schema: *mut xmlSchema);
  pub fn xmlSchemaNewValidCtxt(schema: *mut xmlSchema) -> *mut xmlSchemaValidCtxt;
  pub fn xmlSchemaFreeValidCtxt(ctxt: *mut xmlSchemaValidCtxt);
  pub fn xmlSchemaSetValidStructuredErrors(
    ctxt: *mut xmlSchemaValidCtxt,
    serror: xmlStructuredErrorFunc,
    ctx: *mut c_void,
  );
  pub fn xmlSchemaValidateDoc(ctxt: *mut xmlSchemaValidCtxt, instance: *mut xmlDoc) -> c_int;

  // xmlschemastypes.h
  pub fn xmlSchemaCleanupTypes();

  // xmlerror.h
  pub fn xmlGetLastError() -> *const xmlError;
  pub fn xmlResetLastError();

  // xmlIO.h – fallback for nested schema contexts that lack resourceLoader
  pub fn xmlRegisterInputCallbacks(
    match_func: xmlInputMatchCallback,
    open_func: xmlInputOpenCallback,
    read_func: xmlInputReadCallback,
    close_func: xmlInputCloseCallback,
  ) -> c_int;

  // uri.h
  pub fn xmlBuildURI(uri: *const xmlChar, base: *const xmlChar) -> *mut xmlChar;

  // native/schema_refs.c
  pub fn xsd_collect_schema_locations(
    xml: *const c_char,
    len: c_int,
    url: *const c_char,
    cb: Option<unsafe extern "C" fn(ctx: *mut c_void, location: *const c_char)>,
    ctx: *mut c_void,
  ) -> c_int;
  fn xsd_xml_free(ptr: *mut c_void);
}

/// Free a pointer allocated by libxml2 through the installed `xmlFree` hook.
#[inline]
pub unsafe fn xml_free(ptr: *mut c_void) {
  xsd_xml_free(ptr);
}

#[inline]
pub unsafe fn xml_free_xmlchar(ptr: *mut xmlChar) {
  xml_free(ptr as *mut c_void);
}

#[inline]
pub unsafe fn cstr_to_string(ptr: *const c_char) -> Option<String> {
  if ptr.is_null() {
    return None;
  }
  Some(
    std::ffi::CStr::from_ptr(ptr)
      .to_string_lossy()
      .into_owned(),
  )
}

#[inline]
pub unsafe fn xml_char_to_string(ptr: *const xmlChar) -> Option<String> {
  if ptr.is_null() {
    return None;
  }
  Some(
    std::ffi::CStr::from_ptr(ptr as *const c_char)
      .to_string_lossy()
      .into_owned(),
  )
}
