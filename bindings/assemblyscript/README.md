# MuduDB AssemblyScript Binding

This package is the AssemblyScript guest wrapper for
`mududb:component-shim/guest-api`.

The Rust side is `bindings/rs-shim`, which exports
`mududb:component-shim/shim-api` and internally uses the Rust `mududb` facade
crate. The intended wasm target is P2/component-model composition:

```text
AssemblyScript guest component
  imports mududb:component-shim/types
  imports mududb:component-shim/system

Rust rs-shim component
  exports mududb:component-shim/types
  exports mududb:component-shim/system

component compose
  -> final wasm component
```

AssemblyScript does not implement MuduDB encoding, decoding, type layout, SQL
serialization, or database logic.

## Layout

```text
assembly/
  wit.ts       Low-level WIT import declarations.
  database.ts Database facade: open / close / query / command / batch.
  sql.ts       SqlStmt and ValueList wrappers.
  result.ts    ResultSet and Row wrappers.
  index.ts     Public exports.

wit/
  api.wit
  async-api.wit
```

## Build Core Wasm

```sh
npm install
npm run build
```

Compile the smoke example:

```sh
npx asc example/assembly/index.ts --outFile build/release/example.wasm --optimize
```

The generated core wasm still needs a component adapter that uses
`wit/api.wit` as `guest-api`, then it can be composed with the Rust `rs-shim`
component.
