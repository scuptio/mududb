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
Mudu Transpiler(mtp), transpile source code to Mudu procedure

Usage: mtp.exe [OPTIONS] --input <FILE> --output <FILE> [COMMAND]

Commands:
  rust            Transpile Rust source code
  csharp          Transpile C# source code
  python          Transpile Python source code
  golang          Transpile Go source code
  assemblyscript  Transpile AssemblyScript source code
  help            Print this message or the help of the given subcommand(s)

Options:
  -v, --verbose        Enable verbose output
  -i, --input <FILE>   Input file path
  -o, --output <FILE>  Output file path
  -h, --help           Print help
  -V, --version        Print version
```