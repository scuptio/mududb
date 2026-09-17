# wallet-go

The wallet example guest written in **Go**, compiled to a WASI 0.2 component
with [TinyGo](https://tinygo.org/) and installed into mudud as
`wallet-go.mpk`. It mirrors [wallet-py](../wallet-py) procedure-for-procedure
(`create_user`, `deposit`, `withdraw`, `transfer_funds`, `balance`) and is
the reference for the Go guest toolchain (the mpm-crate `go` template
carries the same pipeline). A sixth procedure, `update_profile`, is the
reference for **user-defined record types**: the `profile`/`address` records
are declared in `wit/types.wit`, generated into `wallet_go/gentypes` by
`mgen message -l go`, and resolved in procedure signatures by
`mtp --type-wit`.

## How it works

```
wit/types.wit  ──mgen message -l go --namespace gentypes──►  gentypes/
                 (Profile/Address structs + XxxToValue/FromValue codecs;
                 --namespace sets the Go package clause, wire.go is the
                 helper runtime mgen copies from the binding)
procedures.go  ──mtp(go) --type-wit wit/types.wit
                        --type-import wallet_go/gentypes──►  main_gen.go
                          (mp2-* adapter, package main)
                          +  main_gen.wit       (world: include
                             wasi:cli/imports@0.2.0, import
                             mududb:api/system, mp2-<kebab> exports)
                          +  package/package.desc.json
mgen check-sql            (SQL literals in procedures.go vs sql/ddl.sql)
tinygo build -target=wasip2 ──► build/wallet_go.wasm ──► wasm-tools validate
mpm-build ──► target/go-guest/wallet-go.mpk
```

- `procedures.go` — business logic. Each `// mudu-proc` function's first
  parameter is the bound session `muduOid` (injected by the adapter from
  `UniProcedureParam.session`); the remaining parameters arrive as plain Go
  values (`int64`, `string`, `float64`, `bool`, `[]byte`) or, for
  user-defined types, as the generated structs (`gentypes.Profile`). A `*T`
  parameter is an optional (nullable) value of the inner type.
- `mudusys.go` — the hand-written syscall layer. There is no Go db facade:
  query/command syscalls are encoded with the in-repo Go binding
  (`types.EncodeQueryRequest` / `types.DecodeQueryResult` / the `command`
  pair) and sent over the `mududb:api/system` byte pipe
  (`binding/mududb/api/system`, wit-bindgen-go output, checked in).
- `go.mod` wires the binding with a `replace` to
  [`../../bindings/go`](https://github.com/ybbh/mududb_p/tree/main/crates/sdk/bindings/go)
  (packages `types` + `codec`: the mgen-generated Uni* types/MSSP func
  codecs and the hand-written MessagePack + MSSP frame runtime), so the
  example always builds against the live binding. The
  `go.bytecodealliance.org/cm` runtime is checked in under `third_party/cm`
  (wit-bindgen-go 0.7.0's runtime, v0.3.0) and wired with a second
  `replace`, so builds run fully offline — no `go mod download`, no vendor
  snapshot of the binding.
- The mtp-generated adapter (`main_gen.go`, a build artifact) assigns one
  `muduworld.Exports.Mp2*` closure per procedure: decode the MessagePack
  `UniProcedureParam`, run the procedure, encode the
  `UniResult<UniProcedureResult, UniError>` reply. A record-typed parameter
  decodes through the binding bridge
  (`types.RecordFieldValues(p.Params[i])` unwraps the positional
  record-case envelope) composed with `gentypes.ProfileFromValue`; a
  record-typed result wraps `gentypes.ProfileToValue(result)` back with
  `types.RecordFromFieldValues`. On the desc/JSON surface the record fields
  keep their WIT names as JSON keys; booleans and 32-bit integers ride as
  i32 (a WIT `bool` is `0`/`1` in the invoke JSON — the host has no boolean
  data-type family).
- `update_profile` stores the record flattened into the `profile` table
  (the nested optional address as the nullable `home_city`/`home_zip`
  columns, the tag list as `profile_tags` side rows keyed by
  `(user_id, idx)`), updating the row by key and inserting it on first
  write.

## Build

Requirements: TinyGo 0.42.0 AND the Go 1.27.1 toolchain on `PATH` (TinyGo
shells out to `go` for package loading), plus `wasm-tools`, `cargo-make`,
and the workspace tools `mgen` / `mtp` / `mpm-build` (`cargo make
install-tools` at the repo root).

```sh
cargo make package    # produces target/go-guest/wallet-go.mpk (repo-root target/)
```

After adding or renaming procedures in `procedures.go`, regenerate the
checked-in wit-bindgen-go bindings to match the refreshed world WIT:

```sh
cargo make regen-bindings   # needs the Go module cache or network once
```

## Test

```sh
# end-to-end (builds a real mudud backend; skips when the mpk is absent):
cd ../../../db-kernel   # crates/db-kernel
cargo test -p testing --test wallet_go_mpk

# the whole suite incl. rebuild:
cargo make test       # in crates/db-kernel/testing
```

## Notes

- Component size is ~1.2 MB and instantiation is fast — the TinyGo route has
  no interpreter cold start (contrast with the componentize-py guest).
- The procedure byte pipe is NOT MSSP-framed: the host passes a bare
  MessagePack `UniProcedureParam` record and expects the single-entry-map
  `UniResult` shape back (`{0: ok}` / `{1: UniError}`); `mudusys.go` encodes
  both with `codec.MpackWriter` and the `types` record codecs.
