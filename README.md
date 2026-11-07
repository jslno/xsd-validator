# @zugferd/xsd-validator

[![CI](https://github.com/jslno/xsd-validator/actions/workflows/CI.yml/badge.svg)](https://github.com/jslno/xsd-validator/actions/workflows/CI.yml)
[![npm version](https://img.shields.io/npm/v/@zugferd/xsd-validator.svg)](https://www.npmjs.com/package/@zugferd/xsd-validator)
[![license](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![node](https://img.shields.io/node/v/@zugferd/xsd-validator.svg)](package.json)

Fast, self-contained **XML Schema (XSD) validation for Node.js**, implemented as a native addon in Rust with a statically vendored [libxml2](https://gitlab.gnome.org/GNOME/libxml2).

It compiles an XSD once — recursively resolving `xs:include`, `xs:import`, `xs:redefine`, and `xs:override` through a loader you provide — then validates XML documents synchronously against the compiled schema. It works with any XSD, and is particularly suited to e-invoicing formats such as ZUGFeRD/Factur-X, XRechnung, and UN/CEFACT Cross Industry Invoice (CII).

## Why use this

- **No system dependencies.** libxml2 is compiled in and statically linked, so validation behaves consistently across platforms without installing libxml2 or a build toolchain at runtime.
- **Prebuilt native binaries.** macOS (x64/arm64), Linux (x64/arm64 GNU), and Windows (x64/arm64) binaries are published as optional dependencies and selected automatically at install time.
- **Async, schema-aware loading.** `XsdValidator.create()` accepts a sync or async loader for external schema references. The loader runs on the Node.js side — never from a libxml2 C callback — and is called once per unique dependency during compilation.
- **Safe concurrency.** Multiple validators can be created concurrently; each schema compile uses its own in-memory virtual filesystem.
- **Compile once, validate many.** Schema compilation is the expensive step. Reuse the resulting `XsdValidator` for fast, synchronous validation of many documents.
- **Useful diagnostics.** Failures report libxml2 messages including file, line, and error details.
- **Network access disabled.** XML and schema parsing use libxml2's `XML_PARSE_NONET`, so validation never fetches remote resources.

## Installation

```bash
npm install @zugferd/xsd-validator
# or
pnpm add @zugferd/xsd-validator
```

Requires Node.js `>=18.17.0 <19 || >=20.3.0 <21 || >=21.1.0` (Node.js LTS recommended). The matching prebuilt binary for your platform is installed automatically; no Rust, CMake, or C compiler is needed.

## Usage

### Validate an XML document

```ts
import { XsdValidator } from '@zugferd/xsd-validator'

const ROOT_XSD = `<?xml version="1.0" encoding="UTF-8"?>
<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema"
           targetNamespace="urn:example"
           xmlns="urn:example"
           elementFormDefault="qualified">
  <xs:element name="Person">
    <xs:complexType>
      <xs:sequence>
        <xs:element name="name" type="xs:string"/>
        <xs:element name="age" type="xs:positiveInteger"/>
      </xs:sequence>
    </xs:complexType>
  </xs:element>
</xs:schema>`

const validator = await XsdValidator.create(ROOT_XSD)

const xml = `<?xml version="1.0" encoding="UTF-8"?>
<Person xmlns="urn:example">
  <name>Ada</name>
  <age>36</age>
</Person>`

console.log(validator.validate(xml)) // true
```

### Load multi-file schemas

Provide a loader when the root XSD references other files. Paths are resolved relative to the schema that references them (e.g. an `xs:include` of `common/types.xsd` inside the root yields `common/types.xsd`; nested includes may yield paths such as `common/base.xsd`).

```ts
import { readFile } from 'node:fs/promises'
import { dirname, join } from 'node:path'
import { XsdValidator } from '@zugferd/xsd-validator'

const schemaDir = '/path/to/schemas'
const rootXsd = await readFile(join(schemaDir, 'CrossIndustryInvoice_100pD22B.xsd'), 'utf8')

const validator = await XsdValidator.create(rootXsd, async (filename) => {
  return readFile(join(schemaDir, filename), 'utf8')
})

const invoice = await readFile('/path/to/invoice.xml', 'utf8')
console.log(validator.validate(invoice))
```

The loader may also be synchronous, and it is only invoked during `XsdValidator.create()` — never during validation.

### Get detailed validation errors

```ts
try {
  validator.validateOrThrow(invoice)
  console.log('valid')
} catch (error) {
  // Multi-line libxml2 diagnostics, e.g. "root.xsd:12: Element 'age': ..."
  console.error(error.message)
}
```

### Batch validation

```ts
const documents = await Promise.all(files.map(async (file) => ({ file, xml: await readFile(file, 'utf8') })))

const valid = documents.filter(({ xml }) => validator.validate(xml))
```

## API

Types are bundled in [`index.d.ts`](index.d.ts).

| API                                     | Description                                                                                                                                                                                                                           |
| --------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `XsdValidator.create(rootXsd, loader?)` | Compiles a root XSD. `loader(filename)` returns the text of each referenced schema and may be sync or async. Returns `Promise<XsdValidator>`. Rejects if references exist without a loader, a loader fails, or the schema is invalid. |
| `validator.validate(xml)`               | Returns `true` when the document is well-formed and schema-valid, `false` otherwise. Throws only on internal failures.                                                                                                                |
| `validator.validateOrThrow(xml)`        | Returns `true` on success; throws an `Error` with libxml2 diagnostics for malformed or invalid XML.                                                                                                                                   |
| `shutdownLibxml()`                      | Optional process-level libxml2 teardown. Normally unnecessary — let the process exit normally, and do not call while validators are still in use.                                                                                     |

### Behavior notes

- `validate()` returns `false` for both malformed XML and schema violations. Use `validateOrThrow()` when you need to distinguish and report the reason.
- If the root XSD has no external references, omit the loader. If references exist and no loader is supplied, `create()` rejects.
- Absolute `schemaLocation` values are passed to your loader as-is; libxml2 will not fetch them over the network.
- A validator holds its compiled schema and schema sources in memory for its lifetime. Discard it when no longer needed.

## Development

### Prerequisites

- [Node.js](https://nodejs.org/) 18.17+ and [pnpm](https://pnpm.io/) (`corepack enable`)
- [Rust](https://www.rust-lang.org/tools/install) stable toolchain
- [CMake](https://cmake.org/) and a C/C++ toolchain (MSVC Build Tools on Windows)
- Git submodules initialized (libxml2 is vendored as a submodule)

### Setup

```bash
git clone --recursive https://github.com/jslno/xsd-validator.git
cd xsd-validator
# If you cloned without --recursive:
git submodule update --init --recursive

corepack enable
pnpm install
pnpm build
pnpm test
```

### Scripts

| Script             | Description                                                                      |
| ------------------ | -------------------------------------------------------------------------------- |
| `pnpm build`       | Build the release native addon with napi-rs                                      |
| `pnpm build:debug` | Build a debug native addon                                                       |
| `pnpm test`        | Run the [AVA](https://github.com/avajs/ava) test suite                           |
| `pnpm bench`       | Run [tinybench](https://github.com/tinylibs/tinybench) benchmarks                |
| `pnpm lint`        | Lint JavaScript/TypeScript with [oxlint](https://oxc.rs/docs/guide/usage/linter) |
| `pnpm format`      | Format with Prettier, `cargo fmt`, and Taplo                                     |

### Project layout

| Path                                           | Purpose                                                        |
| ---------------------------------------------- | -------------------------------------------------------------- |
| [`src/lib.rs`](src/lib.rs)                     | napi-rs bindings and validator lifecycle                       |
| [`src/resolver.rs`](src/resolver.rs)           | Recursive schema resolution and in-memory virtual filesystem   |
| [`src/error.rs`](src/error.rs)                 | libxml2 error collection and JavaScript error conversion       |
| [`native/schema_refs.c`](native/schema_refs.c) | Helper that walks XSD nodes to collect schema references       |
| [`build.rs`](build.rs)                         | Builds the vendored libxml2 and links the native helper        |
| [`vendor/libxml2`](vendor/libxml2)             | libxml2 git submodule                                          |
| [`__test__`](__test__)                         | AVA tests, XRechnung fixtures, and CII schemas                 |
| [`benchmark/bench.ts`](benchmark/bench.ts)     | Validation benchmarks                                          |
| `npm/`                                         | Per-platform npm package manifests generated during publishing |

### Continuous integration

[`.github/workflows/CI.yml`](.github/workflows/CI.yml) runs linting plus a build/test matrix for macOS, Linux, and Windows on x64 and arm64. Releases are published from version tags via [`publish.yml`](.github/workflows/publish.yml).

## Contributing

Contributions are welcome. To propose a change:

1. Open an issue describing the bug or feature if one does not already exist.
2. Fork the repository and create a topic branch.
3. Add or update tests under [`__test__`](__test__), and include a fixture for new behavior where practical.
4. Run `pnpm test`, `pnpm lint`, and `pnpm format` before opening a pull request.
5. Submit the pull request with a clear description and link the related issue.

Please keep pull requests focused and avoid committing build artifacts.

## Getting help

- **Bugs and feature requests:** [GitHub Issues](https://github.com/jslno/xsd-validator/issues)
- **Source and releases:** [github.com/jslno/xsd-validator](https://github.com/jslno/xsd-validator)
- **Native addon tooling:** [napi.rs](https://napi.rs/)
- **Schema engine:** [libxml2](https://gitlab.gnome.org/GNOME/libxml2)

## License

[MIT](LICENSE)
