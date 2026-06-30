# Mudu Transpiler (MTP) – Compile to Mudu Procedures

## Overview

Mudu Transpiler (MTP) is a source-to-source transpiler that transforms code from multiple programming languages into
executable Mudu Procedures – optimized routines that run directly on the MuduDB computational engine. Generated
procedures compile to WebAssembly (WASM) and execute natively within MuduDB.

## Supported Languages

- AssemblyScript
- Rust

## AssemblyScript procedures

AssemblyScript procedures are discovered with a `/**mudu-proc*/` marker. The first parameter must be `Oid`; subsequent parameters are scalar procedure arguments. The return type must be a scalar or `Result<T>`:

```ts
/**mudu-proc*/
export function transfer(id: Oid, account1: i64, account2: i64): Result<i64> {
  return Result.ok<i64>(0);
}
```

Supported scalar parameter and return types: `bool`, `i64`, `f64`, `string`, `Uint8Array`/`bytes`, and `Oid`. When the return type is `Result<T>`, errors are converted to a `MuduResult<ValueList>` by the generated adapter.

> Implementation reference: `mudu_transpiler/src/assemblyscript/parser.rs`, `mudu_transpiler/src/assemblyscript/tests.rs`.

`mtp --input procedure.ts --output procedure.gen.ts assembly-script` writes:

- `procedure.gen.ts`: original AssemblyScript source with generated `adapter_P` exports appended
- `procedure.gen.rs`: generic language procedure shim Rust P2 wrappers such as `mp2_P` and `mudu_inner_p2_P`
- `procedure.gen.wit`: procedure-specific WIT interfaces such as `procedure-p`

## Key Features

### 1. Multi-Language Input

- Write procedures in Rust or AssemblyScript

- Consistent MuduDB API across supported languages

### 2. Async Transformation (Rust)

- Write synchronous code and run asynchronously

- Zero-cost async abstractions for database operations

### 3. WASM Compilation Target

- Outputs standards-compliant WebAssembly

- Sandboxed execution environment and near-native performance

## Mudu Transpiler(mtp) command line

```
Transpiles source code from various programming languages to Mudu procedure format

Usage: mtp [OPTIONS] --input <INPUT> --output <OUTPUT> <COMMAND>

Commands:
  rust             Transpile Rust source code
  assembly-script  Transpile AssemblyScript source code
  help             Print this message or the help of the given subcommand(s)

Options:
  -i, --input <INPUT>
          Input file path

  -o, --output <OUTPUT>
          Output file path

  -m, --module <MODULE>
          MPK module name

  -s, --src-mod <SRC_MOD>
          Source Rust code module name

  -d, --dst-mod <DST_MOD>
          Destination Rust code module name

  -a, --async
          Enable compile to async (Rust-specific)

  -t, --type-desc <TYPE_DESC_FILE>
          Custom type description file

  -v, --verbose
          Enable verbose output

  -p, --package-desc <PACKAGE_DESC>
          Procedure description file

  -h, --help
          Print help (see a summary with '-h')

  -V, --version
          Print version
```
