// Every napi-rs package reads NAPI_RS_WASI_FLAVOR when it loads, including the
// @oxc-node TypeScript loader, so CI passes the flavor under test separately and
// it is applied here, before ../index.js is imported.
if (process.env.XSD_VALIDATOR_WASI_FLAVOR) {
  process.env.NAPI_RS_WASI_FLAVOR = process.env.XSD_VALIDATOR_WASI_FLAVOR
}
