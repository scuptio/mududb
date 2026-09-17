# wallet-c

The wallet example guest written in **C**, compiled to a WebAssembly
component with a freestanding clang build and `wasm-tools`, and installed
into mudud as `wallet-c.mpk`. It mirrors
[wallet-py](../wallet-py) procedure-for-procedure (`create_user`,
`deposit`, `withdraw`, `transfer_funds`, `balance`) and exercises the
formal C binding ([`../../bindings/c`](https://github.com/ybbh/mududb_p/tree/main/crates/sdk/bindings/c))
live — the same role wallet-go plays for the Go binding (the mpm-crate `c`
template carries the same pipeline with a minimal self-contained syscall
layer instead). `update_profile` demonstrates a **user-defined record
type** (`wit/types.wit`, generated into `gentypes/Types.h` by mgen) as a
procedure parameter and return type.

## How it works

```
src/procedures.c ──mtp(c)──►  generated/procedures_gen.c   (mp2-* export
                             wrappers + arity/kind preambles)
                          +  generated/procedures_gen.wit  (world: import
                             mududb:api/system, mp2-<kebab> exports)
                          +  package/package.desc.json
wit/types.wit   ──mgen message──► gentypes/Types.h         (record structs
                             + xxx_encode/xxx_decode codecs; checked in)
                 ──mtp --type-wit──►  (registers the record names so
                             procedure annotations may use them;
                             --type-import "gentypes/Types.h")
mgen check-sql             (SQL literals in procedures.c vs sql/ddl.sql)
clang --target=wasm32 -nostdlib ──► build/wallet_c.core.wasm
wasm-tools component embed/new ──► build/wallet_c.wasm ──► wasm-tools validate
mpm-build ──► target/c-guest/wallet-c.mpk
```

- `src/procedures.c` — business logic. Each `// mudu-proc (name: type,
  ...) -> type` function is a non-`static` `mudu_proc_fn` (`int fn(const
  mudu_proc_param *, mudu_datum *, mudu_error *)`); the session OID is
  implicit in `param->session`, the remaining parameters arrive
  positionally as `mudu_datum`s. Annotation types: `i64`, `f64`, `string`,
  `option<T>` (nullable), and the user-defined record/enum names from
  `wit/types.wit` — the `mudu_datum` support surface of `mudu_sys.h`
  (a record arrives as `MUDU_DATUM_RECORD`, an enum as an i32 ordinal in
  `MUDU_DATUM_I64`).
- `update_profile` decodes its record argument with two calls: the
  binding's record bridge (`mp_record_bridge_write` in
  `mududb/codec/record_bridge.h`) serializes the positional
  `uni-data-value` record envelope as the integer-keyed MessagePack map the
  generated `profile_decode` reads; the return path is symmetric
  (`profile_encode` + `mp_record_bridge_read`). Bool fields cross as i32
  0/1 both directions (`mpr_bool` accepts integer markers 0/1 for exactly
  this); the nested optional address flattens into nullable columns and the
  tag list lands in a side table — see the function comments.
- `src/mudu_sys.{h,c}` — the hand-written syscall layer. There is no C db
  facade: query/command syscalls are encoded and decoded with the formal C
  binding (`uni_syscall_query_request_encode` /
  `uni_syscall_query_result_decode` and the `command` pair from
  `mududb/types/UniSyscall.h`, over `mududb/codec/mpack.{h,c}`) and sent
  over the `mududb:api/system` byte pipe.
- The binding is compiled **straight from source** (`Makefile.toml`
  `BINDING_DIR` adds `../../bindings/c` to the include path and puts
  `mududb/codec/mpack.c` + `mpack_alloc.c` + `record_bridge.c` on the
  clang source list), so
  the example always builds against the live binding — the C analogue of
  wallet-go's `replace` in `go.mod`; nothing is vendored. The binding's
  `mp_realloc` allocation hook (mpack_alloc.c) lands on the freestanding
  `realloc`/`free` shims in `src/mudu_sys.c`, backed by the guest bump
  arena — decoded values live until the guest call ends, per the binding's
  ownership contract.
- The mtp-generated adapter (`generated/procedures_gen.c`, a build
  artifact) wraps each procedure: check arity/kind against the annotation,
  decode the MessagePack `UniProcedureParam`, run the procedure, encode
  the `UniResult<UniProcedureResult, UniError>` reply.

## Build

Requirements: a clang able to target **wasm32 freestanding** — the
[wasi-sdk](https://github.com/WebAssembly/wasi-sdk) 29 clang is used by
default (`~/.wasi-sdk/wasi-sdk-29.0`; override with
`WASI_SDK=/path/to/wasi-sdk cargo make`, or `CLANG=/path/to/clang`) — plus
`wasm-tools`, `cargo-make`, and the workspace tools `mgen` / `mtp` /
`mpm-build` (`cargo make install-tools` at the repo root). Only the
compiler and the sysroot *headers* are used: the link is `-nostdlib` and
the component imports nothing but `mududb:api/system`.

```sh
cargo make package    # produces target/c-guest/wallet-c.mpk (repo-root target/)
```

## Test

```sh
# end-to-end (builds a real mudud backend; skips when the mpk is absent):
cd ../../../db-kernel   # crates/db-kernel
cargo test -p testing --test wallet_c_mpk

# the whole suite incl. rebuild:
cargo make test       # in crates/db-kernel/testing
```

## Notes

- Component size is ~38 KB and instantiation is fast — the freestanding C
  route has no runtime at all (contrast with the TinyGo and
  componentize-py guests).
- The procedure byte pipe is NOT MSSP-framed: the host passes a bare
  MessagePack `UniProcedureParam` record and expects the single-entry-map
  `UniResult` shape back (`{0: ok}` / `{1: UniError}`); `src/mudu_sys.c`
  decodes with `uni_procedure_param_decode` and encodes with
  `uni_procedure_result_encode` / `uni_error_encode`.
- `cc-build` omits the mpm-crate C template's `-mexec-model=reactor` as
  redundant: `-Wl,--no-entry` already yields the reactor model.
  `-fno-builtin` stays — it stops the compiler from rewriting the
  freestanding `memcpy` & friends into calls of themselves.
