# Mudu Transpiler (MTP) – Compile to Mudu Procedures

## Overview

Mudu Transpiler (MTP) is a source-to-source transpiler that transforms code from multiple programming languages into
executable Mudu Procedures – optimized routines that run directly on the MuduDB computational engine. Generated
procedures compile to WebAssembly (WASM) and execute natively within MuduDB.

## Supported Languages

- AssemblyScript
- Rust
- Python
- C#
- C
- Go

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

## Python procedures

Python procedures are top-level `def` functions discovered with a `# mudu-proc` marker comment:

```py
# mudu-proc
def create_user(session: UniOid, user_id: int, name: str) -> int:
    ...
```

When the first parameter is named `session` (unannotated or annotated `UniOid`/`Oid`), it is the bound
session: the generated adapter injects `UniProcedureParam.session` for it and the wire `param_list`
(and the package desc) carry only the remaining parameters — the same convention as the
AssemblyScript guest's leading `Oid` parameter. Other parameters arrive positionally, unwrapped from
`UniDataValue` to plain Python values (`bool`/`int`/`float`/`str`/`bytes`/`None`). Type hints are
advisory metadata for the procedure descriptor only; unannotated parameters map to a permissive
default (String family). Return `None` for no result, a single value, or a tuple for multiple
results; plain values are wrapped back into `UniDataValue` by the adapter.

`mtp --input procedures.py --output procedures_gen.py --module wallet-py python` writes:

- `procedures_gen.py`: the componentize-py adapter module — a `WitWorld(wit_world.WitWorld)`
  subclass with one `mp2_<snake>` byte-pipe method per procedure, plus a top-level wiring block that
  installs `wit_world.imports.system` as the `mududb.sys` syscall transport
- `procedures_gen.wit`: the procedure world (`import mududb:api/system` plus one root-level
  `mp2-<kebab>` export per procedure)
- with `--package-desc`: the procedure descriptor JSON (same `ModProcDesc` schema as the other
  front-ends)

> Implementation reference: `mudu_transpiler/src/python/parser.rs`, `mudu_transpiler/src/python/tests.rs`.

## C# procedures

C# procedures are `public static` methods discovered with a `// mudu-proc` line comment placed
immediately above the method (only whitespace between the comment and the method):

```cs
// mudu-proc
public static long CreateUser(MuduOid session, long userId, string name, string email)
{
    ...
}
```

The first parameter must be the session OID (`MuduOid`); subsequent parameters are scalar
procedure arguments (`long` → I64, `string` → String, `double` → F64, `bool` → I32,
`byte[]` → Binary). The return type must be `long` (the wallet-cs result convention: one i64
datum). PascalCase method names and camelCase parameter names normalize to the snake_case wire
form (`CreateUser` → `create_user`, `userId` → `user_id`).

`mtp --input Procedures.cs --output WalletCsWorldExportsImpl.cs --module wallet_cs csharp`
(`cs` alias works) writes:

- `WalletCsWorldExportsImpl.cs`: the componentize-dotnet exports class the wit-bindgen C#
  backend calls by naming convention — namespace `<PascalWorld>World`, class
  `<PascalWorld>WorldExportsImpl`, one `Mp2<PascalProc>` method per procedure
- `WalletCsWorldExportsImpl.wit`: the procedure world (same shape as the other byte-pipe
  front-ends)
- with `--package-desc`: the procedure descriptor JSON

> Implementation reference: `mudu_transpiler/src/csharp/parser.rs`, `mudu_transpiler/src/csharp/tests.rs`.

## C procedures

C procedures are non-`static` functions with the fixed `mudu_proc_fn` ABI
(`int fn(const mudu_proc_param *, mudu_datum *, mudu_error *)`), discovered with a marker
comment that carries the wire signature (the C declaration itself carries no usable type
information):

```c
// mudu-proc (item_id: i64, name: string) -> i64
int create_item(const mudu_proc_param *param, mudu_datum *result, mudu_error *err) {
    ...
}
```

Annotation types: `i64` → I64 (`MUDU_DATUM_I64`), `f64` → F64 (`MUDU_DATUM_F64`),
`string` → String (`MUDU_DATUM_STR`) — the `mudu_datum` support surface of the C guest
syscall layer. The session OID is implicit in `mudu_proc_param.session` and is not part of
the annotated parameters.

`mtp --input procedures.c --output procedures_gen.c --module demo_c c` writes:

- `procedures_gen.c`: per procedure an `extern` declaration, a static arity/kind check
  preamble, and the `mp2-<kebab>` export wrapper driving `mudu_run_proc`
- `procedures_gen.wit` and, with `--package-desc`, the procedure descriptor JSON

> Implementation reference: `mudu_transpiler/src/c/parser.rs`, `mudu_transpiler/src/c/tests.rs`.

## Go procedures

Go procedures are top-level functions discovered with a `// mudu-proc` line comment placed
immediately above the function; the signature is parsed directly:

```go
// mudu-proc
func createItem(session muduOid, itemID int64, name string) (int64, error) {
    ...
}
```

The first parameter must be the session OID (`muduOid`); the result must be `(T, error)` with
`T` one of `int64` → I64, `string` → String, `float64` → F64, `bool` → I32, `[]byte` → Binary.

`mtp --input procedures.go --output main_gen.go --module demo_go go` (`golang` alias works)
writes:

- `main_gen.go`: the `package main` wiring module — one `muduworld.Exports.Mp2<PascalProc>`
  assignment per procedure, the shared `invoke` driver, and an empty `main`
- `main_gen.wit`: the procedure world, including the `include
  wasi:cli/imports@0.2.0;` line the TinyGo runtime needs
- with `--package-desc`: the procedure descriptor JSON

> Implementation reference: `mudu_transpiler/src/go/parser.rs`, `mudu_transpiler/src/go/tests.rs`.

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
  python           Transpile Python source code
  csharp           Transpile C# source code
  c                Transpile C source code
  go               Transpile Go source code
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
