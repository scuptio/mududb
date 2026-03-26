# Mudu Transpiler (MTP) – Compile to Mudu Procedures

## Overview

Mudu Transpiler (MTP) is a source-to-source transpiler that transforms code from multiple programming languages into
executable Mudu Procedures – optimized routines that run directly on the MuduDB computational engine. Generated
procedures compile to WebAssembly (WASM) and execute natively within MuduDB.

## Supported Languages

- AssemblyScript (in progress)
- C# (in progress)
- Golang (in progress)
- Python (in progress)
- Rust (currently)

## Key Features

### 1. Multi-Language Input

- Write procedures in familiar languages

- Consistent MuduDB API across all languages

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
  c-sharp          Transpile C# source code
  python           Transpile Python source code
  golang           Transpile Go source code
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
