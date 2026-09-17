# Guest/Host Syscall ABI (MSSP v1): Integration Guide

This guide explains how MuduDB guests (stored procedures compiled to
WebAssembly components) talk to the host kernel through the guest→host
syscall ABI, and how to integrate a new guest language with it.

The normative, versioned specification of the wire format is the contract
document [`../contract/syscall_payload_v1.md`](../contract/syscall_payload_v1.md)
(中文: [`../../cn/contract/syscall_payload_v1.md`](../../cn/contract/syscall_payload_v1.md)).
This guide is the practical companion: architecture, per-language entry
points, and how to verify a new implementation.

## Overview

Every syscall a guest makes — SQL, key/value, relation, filesystem — crosses
the component boundary as an **opaque `list<u8>`**. The bytes are an MSSP v1
frame: a 16-byte big-endian header plus a MessagePack body. The host router
decodes the frame, dispatches on the message kind, and encodes the result as
another MSSP frame.

```text
+---------------------+        +----------------------+        +-------------------+
| Guest language      |        | Component boundary   |        | Host              |
|                     |        |                      |        |                   |
|  workspace Rust     |        |  WIT functions in    |        |  mudu_runtime     |
|  standalone Rust    | MSSP   |  uni-syscall.wit     | MSSP   |  kernel_sync /    |
|  C#                 | frame  |  (opaque list<u8>    | frame  |  kernel_async     |
|  AssemblyScript     |=======>|  in / list<u8> out)  |=======>|        |          |
|                     |        |                      |        |  syscall_payload  |
|                     |        |                      |        |  router (23 kinds)|
+---------------------+        +----------------------+        +--------+----------+
                                                                        |
                                                               +--------v----------+
                                                               | mudu_kernel       |
                                                               | (SQL/KV/relation/ |
                                                               |  fs execution)    |
                                                               +-------------------+
```

- The WIT boundary is defined in
  [`uni-syscall.wit`](../../../crates/common/mudu_binding/wit/uni-syscall.wit)
  (23 functions); the runtime world that imports it is
  [`mudu_runtime/wit/api.wit`](../../../crates/db-kernel/mudu_runtime/wit/api.wit).
- The host-side framed codec (authoritative implementation) lives in
  [`mudu_binding/src/codec/syscall_payload/`](../../../crates/common/mudu_binding/src/codec/syscall_payload/)
  (`mod.rs` for framing, `router.rs` for per-kind encode/decode).
- The exception is `fetch`: a host WIT function that is **not** routed
  through MSSP; it exchanges raw `mp_wire` bytes (see
  [The `fetch` protocol](#the-fetch-protocol) below).

## MSSP v1 frame format

Each request and each response is one self-describing frame. There is no
length prefix; the WIT transport delivers the exact byte range.

```text
+--------------------------------+
| Header (16 bytes, big-endian)  |
+--------------------------------+
| Body (variable, MessagePack)   |
+--------------------------------+
```

### Header

| Offset | Size | Field | Description |
|--------|------|-------|-------------|
| 0 | 4 | `magic` | `0x4D53_5350` (ASCII `MSSP`). |
| 4 | 4 | `version` | Payload format version. Current: `1`. |
| 8 | 4 | `flags` | Reserved. Must be `0`; any non-zero value is rejected. |
| 12 | 4 | `message_kind` | Message-kind discriminant, 1–23 (table below). |

### Body rules

- The body is a single MessagePack value; decoders **reject trailing bytes**.
- **Requests** are MessagePack maps keyed by the 1-based WIT parameter
  numbers (declaration order).
- **Results** are `[0u8, value]` for `ok` and `[1u8, UniError]` for `err`.
- **Byte blobs** (`list<u8>` parameters and results) are MessagePack bin.
- Records are MessagePack maps keyed by the 1-based field numbers
  (declaration order); variants are
  `[tag, payload]` pairs; `option<T>` is nil or the value. The full type
  tables (`UniScalarValue`, `UniDataValue`, `UniDataType`, composite layouts)
  are in the [contract document](../contract/syscall_payload_v1.md).

### Pinned wire quirks

These behaviors are part of v1 and are verified byte-exact by the golden
corpus; independent codecs must match them:

- `UniError.err_details` and `UniResultSet.cursor` are `Vec<u8>` fields that
  encode as MessagePack **arrays** of integers (the record-context `list<u8>`
  rule), **not** bin.
  Every other `list<u8>` field uses bin.
- `rmp_serde` integer compaction: the shortest integer form wins. Small
  non-negative integers are fixints (`0` → `0x00`); small negative integers
  are negative fixints (`i64 -2` → `0xFE`). Blobs shorter than 256 bytes use
  bin8 (`0xC4`).
- A host error without a source carries `err_src` as the JSON string
  `"None"`.
- fs errno mapping: host error code `50029`
  (`ErrorCode::InvalidArgument`) maps to guest-facing `EINVAL` (`22`) via
  `sys_interface::fs::map_fs_errno`.

## Syscall map

The 23 syscalls, their WIT functions, and their message kinds:

| Kind | WIT function | Category | Result payload |
|------|--------------|----------|----------------|
| 1 | `query` | SQL | `UniQueryResult` |
| 2 | `command` | SQL | `UniCommandResult` |
| 3 | `batch` | SQL | `UniCommandResult` |
| 4 | `open-session` | Session | `UniOid` (session OID) |
| 5 | `close-session` | Session | unit |
| 6 | `get` | KV | `option<list<u8>>` |
| 7 | `put` | KV | unit |
| 8 | `delete` | KV | unit |
| 9 | `range` | KV | `list<tuple<list<u8>, list<u8>>>` |
| 10 | `fs-open` | Filesystem | `u32` (fd) |
| 11 | `fs-close` | Filesystem | unit |
| 12 | `fs-read` | Filesystem | `list<u8>` |
| 13 | `fs-write` | Filesystem | `u32` |
| 14 | `fs-pread` | Filesystem | `list<u8>` |
| 15 | `fs-pwrite` | Filesystem | unit |
| 16 | `fs-lseek` | Filesystem | `u64` |
| 17 | `fs-fstat` | Filesystem | `UniFsStat` |
| 18 | `fs-stat` | Filesystem | `UniFsStat` |
| 19 | `fs-fsync` | Filesystem | unit |
| 20 | `fs-readdir` | Filesystem | `list<UniFsDirent>` |
| 21 | `relation-get` | Relation | `option<list<option<list<u8>>>>` |
| 22 | `relation-update` | Relation | `u64` (affected rows) |
| 23 | `relation-insert` | Relation | unit |

Request shapes for every kind are tabulated in the
[contract document](../contract/syscall_payload_v1.md#message-kinds).

**Exception: `fetch`.** `fetch(query-result: list<u8>) -> list<u8>` is
declared in the runtime world (`mudu_runtime/wit/api.wit`) as a host
function that exchanges raw `mp_wire` bytes directly. It does **not** go
through MSSP framing; see [The `fetch` protocol](#the-fetch-protocol) below.

## The `fetch` protocol

`fetch` is implemented on the host
(`mudu_runtime::interface::kernel_sync::fetch_internal`) and mirrors
[`sys_interface::api_impl`](../../../crates/common/sys_interface/src/api_impl/mod.rs),
which remains the wire-shape source of truth. The wire format is raw
`mp_wire` with no MSSP header:

- **Request:** the `mp_wire` encoding of a `UniOid` (a field-number map) —
  exactly the
  bytes carried in `UniResultSet.cursor` of a query response.
- **Response:** the `mp_wire` encoding of
  `UniResult<UniResultSet, UniError>`. Every failure is encoded into the
  `UniResult::Err` envelope because the WIT ABI returns a bare `list<u8>`
  with no error channel.

Semantics:

- `fetch` drains **all** cached rows of the cursor's result set and always
  reports `eof: true`.
- A `fetch` after the result set is drained returns an empty `row_set` with
  `eof: true` — not an error.
- An unknown cursor returns `UniResult::Err` with
  `ErrorCode::EntityNotFound`; a malformed cursor returns `UniResult::Err`.
- Queries on synchronous connections cache their result set on the session
  `Context` so `fetch` can drain it later. Queries on asynchronous
  connections are never cached, so a `fetch` after an async query returns an
  empty `row_set` with `eof: true`.
- Cache lifecycle: the cache is drained destructively; `Context::query_next`
  clears it at EOF; `Context::remove` on session close drops any undrained
  cache.

## Codec generation

All seven codec stacks are **generated by `mgen` from the WIT schema** — the
WIT files under
[`mudu_binding/wit/`](../../../crates/common/mudu_binding/wit/) are the single
source of truth. The project deliberately did **not** create a shared
`mudu_syscall_codec` crate: each consumer (host `mudu_binding`, standalone
Rust SDK, C# SDK, AssemblyScript package, Python package, C binding, Go
binding) vendors its own generated code, and
drift is guarded by the two golden corpora below instead of by a shared
dependency. This keeps the standalone SDK and the guest packages free of
host-crate dependencies while guaranteeing byte parity by construction (same
WIT, same generator) plus byte-exact verification.

### Regeneration commands

From `crates/tools` (Rust — host `mudu_binding` and the standalone Rust SDK
use the same command):

```sh
cargo run -p mudu_gen -- message \
    -i ../common/mudu_binding/wit \
    -o <scratch dir> \
    -l rust --with-func-codec          # or: -l csharp / -l assemblyscript / -l python / -l c / -l go
```

`mgen message` also supports `-t/--type-desc <path>`: alongside the generated
code it emits a schema descriptor JSON (`UniSchemaDesc` — every message kind,
record field number, and variant tag, for runtime reflection and tooling).
The checked-in copy is
[`mudu_binding/src/codec/syscall_payload/syscall_schema.desc.json`](../../../crates/common/mudu_binding/src/codec/syscall_payload/syscall_schema.desc.json),
embedded via `include_str!` as `syscall_payload::SYSCALL_SCHEMA_DESC_JSON`;
the host-side regeneration command (with `-t`) is in
[`mudu_binding/src/codec/syscall_payload/REGENERATE.md`](../../../crates/common/mudu_binding/src/codec/syscall_payload/REGENERATE.md).

- **Rust (host):** copy `uni_syscall.rs` to
  `mudu_binding/src/codec/syscall_payload/generated/` and the `uni_<name>.rs`
  DTOs to `mudu_binding/src/universal/`; details in
  [`mudu_binding/src/codec/syscall_payload/REGENERATE.md`](../../../crates/common/mudu_binding/src/codec/syscall_payload/REGENERATE.md).
- **Rust (SDK):** copy `uni_syscall.rs` to
  `mudu_api/rust/src/mudu_sys/generated/` and the DTOs to
  `mudu_api/rust/src/universal/`, then `cargo fmt`; details in
  [`mudu_api/rust/REGENERATE.md`](../../../crates/sdk/mudu_api/rust/REGENERATE.md).
- **C#:** copy `Uni*.cs` to `mudu_api/csharp/uni/`; install `UniSyscall.cs`
  into `mudu_api/csharp/mudu_sys/` with the namespace rewritten from
  `Universal` to `Mudu.Api.MuduSys` (a `sed` one-liner); exact steps in the
  regen section of
  [`mudu_api/csharp/README.md`](../../../crates/sdk/mudu_api/csharp/README.md).
- **AssemblyScript:** copy all `*.ts` to
  `bindings/assemblyscript/assembly/generated/`, then rewrite the runtime
  import with `sed -i 's|from "./mpack"|from "../mpack"|'`; details in
  [`assembly/generated/README.md`](../../../crates/sdk/bindings/assemblyscript/assembly/generated/README.md).
- **Python:** copy all `*.py` to `bindings/python/mududb/generated/` (the template
  already emits `from mududb.codec.mpack import ...`, so no import rewriting is
  needed), and refresh `mududb/generated/syscall_schema.desc.json` from the host
  codec path when the schema changes; details in
  [`bindings/python/README.md`](../../../crates/sdk/bindings/python/README.md).
- **C:** copy `Uni*.h` to `bindings/c/mududb/types/` (the generated
  `#include "mududb/..."` paths already match the checked-in layout, so no
  rewriting is needed; `mududb/codec/` is hand-written); details in
  [`bindings/c/README.md`](../../../crates/sdk/bindings/c/README.md).
- **Go:** copy `Uni*.go` to `bindings/go/types/` (`types/wire.go` is
  hand-written — never overwrite it); details in
  [`bindings/go/README.md`](../../../crates/sdk/bindings/go/README.md).

### WIT pinning rules

These properties of the WIT schema are deployed wire values — never change
them casually:

- `uni-syscall.wit` declaration order **is** the wire `message_kind`
  numbering (1–23): the `relation-*` functions follow the `fs-*` functions.
- `uni-scalar.wit` discriminants are pinned: `u128` follows `u64` (tag 8),
  `i128` follows `i64` (tag 10), `string` is 14, and its last case is
  `timestamp-tz` (`TimestampTz`, tag 20). `uni-scalar-value.wit` uses the
  same numbering for those cases and additionally ends with `null`
  (`Null`, tag 21) — the payload-less SQL NULL value, encoded as
  `[21, 0u8]`. The C# side pins these with
  `Mudu.Api.Tests/UniScalarTagTests.cs` (a regression test for a tag-skew
  bug found during bring-up).
- `uni-query-argv.wit` and `uni-command-argv.wit` end with
  `param-desc: option<uni-record-type>` (record map key 4): the sender's
  parameter type descriptor. WIT option fields are **omitted from the
  encoded map when they hold no value** (proto3 presence semantics), so a
  frame without a descriptor is byte-identical to the pre-addition form;
  receivers treat a missing key as "no descriptor" and skip the
  descriptor/value consistency check.
- `uni-sql-param.wit` ends with `param-names: option<list<string>>`
  (record map key 2): the names of named (`:name`) placeholders in
  declaration order, parallel to the values. The host rewrites `:name`
  placeholders to positional form and expands values per occurrence (a
  repeated `:name` reuses its value); an absent field is likewise omitted
  from the encoded map and changes nothing on the wire.
- `uni-data-type.wit` ends with `%box(box<uni-data-type>)` (tag 8, after
  `binary` at tag 7). The `Box` variant doubles as `mgen`'s internal IR for
  WIT `box<T>` types, so it must stay even though no syscall payload carries
  it.
- The legacy checksum file `mudu_binding/wit/contract.md5.txt` predates these
  corrections, is stale, and is no longer referenced by any build task.

### Verification gates

Any regeneration must pass both corpora (see
[Interop verification](#interop-verification) and
[MP primitive alignment](#mp-primitive-alignment); the former is semantic +
roundtrip, the latter byte-exact):

1. The 47-frame syscall corpus `syscall_payload_v1_all.bin` — verified by the
   host router, the Rust SDK, the C# SDK, the generated AssemblyScript
   codec (172 checks), and the generated Python codec (188 checks).
2. The 44-vector primitive corpus `mp_primitives_v1.bin|.json` — verified by
   `rmp_serde`, MessagePack-CSharp, `mpack.ts`, and `mpack.py`.

If regenerated code changes any frame byte, reconcile the WIT or the `mgen`
templates — never "fix" the fixtures.

## Per-language integration

### Workspace Rust guests

Nothing to do. Procedures written against the in-repo crates go through
`sys_interface` → `mudu_binding`, whose universal types and per-function
codec are mgen-generated (`codec/syscall_payload/generated/uni_syscall.rs`)
with thin `MuduError` adapters in `mod.rs`/`router.rs`. All 23 kinds are
covered automatically.

### Standalone Rust SDK (`mudu_api_rust`)

[`crates/sdk/mudu_api/rust`](../../../crates/sdk/mudu_api/rust/) is a
standalone cargo workspace (package `mudu_api_rust`) for guests built outside
the main repo. It cannot depend on `mudu_binding`, so it vendors the same
mgen output — universal DTOs in `src/universal/` plus the per-function codec
in `src/mudu_sys/generated/uni_syscall.rs` — with hand-written glue around
it:

- `src/mudu_sys/batch.rs`, `session.rs`, `kv.rs`, `relation.rs`, `fs.rs` —
  the public per-category `serialize_*` / `deserialize_*_result` API and
  `map_fs_errno`, routing through the generated codec.
- Typed async `Mudu` wrappers on top: `open_session`, `get`, `put`,
  `delete`, `range`, `relation_get`, `relation_update`, `relation_insert`,
  `fs_*`, and the SQL calls.
- `--features mock-sqlite` builds an in-process mock with in-memory
  KV/relation/fs emulation, so procedure logic can be unit-tested on the
  host without a running `mudud`.

### C# SDK

[`crates/sdk/mudu_api/csharp`](../../../crates/sdk/mudu_api/csharp/):

- `uni/*.cs` — 22 mgen-generated universal DTOs.
- `mudu_sys/UniSyscall.cs` — mgen-generated MSSP frame codec and
  per-function stubs (namespace rewritten to `Mudu.Api.MuduSys`).
  Resolver-free on the default path, so it runs on the wasi-wasm NativeAOT
  target; scalar tags are pinned against the host by
  `Mudu.Api.Tests/UniScalarTagTests.cs`.
- `mudu_sys/MuduSysCallApi.cs` — hand-written typed syscall surface covering
  all 23 kinds: `SysBatch`, `SysOpen`, `SysClose`, `SysGet`, `SysPut`,
  `SysDelete`, `SysRange`, `SysRelationGet`, `SysRelationUpdate`,
  `SysRelationInsert`, plus the existing query/command/fs calls.
- `mock/MockSqliteMuduSysCall.cs` with `MockKvEmulation` /
  `MockRelationEmulation` / `MockFsEmulation` for host-side unit tests.
- xunit tests in `Mudu.Api.Tests/`:

```sh
~/.dotnet/dotnet test Mudu.Api.Tests
```

**Real C# guests are proven.** [`crates/sdk/example/wallet-cs`](../../../crates/sdk/example/wallet-cs/)
is a byte-pipe C# port of the wallet example (5 procedures, assertion parity
with `wallet-as`), built with componentize-dotnet (.NET 10 SDK,
`dotnet-experimental` feed, `IlcExportUnmanagedEntrypoints`, `wit/deps`
layout) and packaged as `wallet-cs.mpk`. It passes
[`testing/tests/wallet_cs_mpk.rs`](../../../crates/db-kernel/testing/tests/wallet_cs_mpk.rs)
against a real `mudud`. Build requirements and the exact toolchain are
documented in the
[wallet-cs readme](../../../crates/sdk/example/wallet-cs/readme.md). Runtime
notes: sync-world components run on a dedicated runtime-free thread
(`WTInstancePre.requires_async` is computed from the component's imports;
async-importing components keep the `call_async` path), and the TCP invoker
routes per app on `enable_async`/`use_async`.

### AssemblyScript

AssemblyScript guests use the same direct byte-pipe architecture as the C#
guest: the component imports `mududb:api/system` and frames MSSP itself with
the mgen-generated, corpus-verified codec in
[`bindings/assemblyscript/assembly/generated/`](../../../crates/sdk/bindings/assemblyscript/assembly/generated/)
(24 files) plus the `assembly/mpack.ts` runtime. `mtp assembly-script` emits a
per-procedure `mp2_<name>` byte-pipe adapter (decode `UniProcedureParam` →
positional typed arguments → call the user function → encode
`UniResult<UniProcedureResult, UniError>`) and the procedure world WIT
(`import mududb:api/system` plus one root-level `mp2-<kebab>` export per
procedure), so no companion Rust component is involved (see
[`crates/sdk/example/wallet-as/Makefile.toml`](../../../crates/sdk/example/wallet-as/Makefile.toml)):

```text
AssemblyScript component (mtp adapter + generated MSSP codec)
        |
        |  imports mududb:api/system, exports root-level mp2-*
        v
wasm-tools component embed/new  ->  wallet-as component (.mpk)
```

The end-to-end path is proven by
[`testing/tests/wallet_as_mpk.rs`](../../../crates/db-kernel/testing/tests/wallet_as_mpk.rs),
which installs `wallet-as.mpk` on a real `mudud` (Tokio mode) and invokes
`create_user` / `deposit` / `withdraw` / `transfer_funds` / `balance` over
HTTP — including an overdraft error returned to the caller via MSSP.

### Python (tooling / corpus verification)

[`crates/sdk/bindings/python`](../../../crates/sdk/bindings/python/) is the
Python counterpart of the AssemblyScript `assembly/generated/` package: its
role is **tooling and test verification**, not a guest path. Pure stdlib,
Python 3.9+, zero install (every test entry bootstraps `sys.path` itself).
Layout:

- `mududb/codec/mpack.py` — handwritten MessagePack runtime, byte-exact with
  rmp_serde 1.3.x canonical encoding (minimal-width integers by value,
  str8/bin8 thresholds) and lenient on decode; the `F32` marker class makes
  the writer emit f32 (`0xCA`) instead of f64 (`0xCB`).
- `mududb/generated/` — the mgen Python backend output (`-l python
  --with-func-codec`, 24 files including the `uni_syscall.py` frame codecs)
  plus a copy of `syscall_schema.desc.json`.
- `tests/test_mpack.py` — unittests for `mududb.codec.mpack` (36 cases).
- `corpus_common.py` plus three corpus runners — consuming the same
  host-generated golden fixtures as Rust/C#/AS (see
  [Interop verification](#interop-verification) and
  [MP primitive alignment](#mp-primitive-alignment)).

Regenerate (from `crates/tools`, then copy `<scratch dir>/*.py` into
`generated/`):

```sh
cargo run -p mudu_gen -- message \
    -i ../common/mudu_binding/wit \
    -o <scratch dir> \
    -l python --with-func-codec
```

Test commands (in `crates/sdk/bindings/python`):

```sh
python3 -m unittest discover tests   # mpack unit tests
python3 run_mp_corpus_test.py        # 44 primitive vectors (132 checks)
python3 run_syscall_corpus_test.py   # 47 MSSP frames (188 checks)
python3 run_lenient_decode_test.py   # 8 non-canonical frames (16 checks)
```

### C (corpus verification)

[`crates/sdk/bindings/c`](../../../crates/sdk/bindings/c/) is the C
counterpart of the Python binding: its role is **corpus verification** plus
a reference for C guests. C99/C++17 dual-clean, zero dependencies beyond
libc. Layout:

- `mududb/codec/mpack.{h,c}` — hand-written MessagePack runtime (canonical
  writer, lenient reader, sticky error flags, `mp_arena` bump allocator:
  decoded values never point into the input buffer; one `mpa_free`
  releases everything).
- `mududb/types/Uni*.h` — the mgen C backend output (`-l c
  --with-func-codec`, 23 self-contained headers including the `UniSyscall.h`
  frame codecs). Cross-file reference cycles are broken by forward
  declarations, pointer payloads for named record/variant cases, and
  two-phase (early/late) includes; `MP_INLINE` keeps the headers clean
  under both C99 and C++.
- `tests/corpus_driver.c` — native corpus runner (no WASM): the same three
  suites as the Python runners, 398 checks total, built and run by
  `make check` (also the per-header C99 + C++17 compile gates).

Regenerate (from `crates/tools`, then copy `<scratch dir>/Uni*.h` into
`mududb/types/`):

```sh
cargo run -p mudu_gen -- message \
    -i ../common/mudu_binding/wit \
    -o <scratch dir> \
    -l c --with-func-codec
```

Test command (in `crates/sdk/bindings/c`):

```sh
make check CC=clang CXX=clang++   # header compile gates + corpus driver (398 checks)
```

### Go (corpus verification)

[`crates/sdk/bindings/go`](../../../crates/sdk/bindings/go/) is the Go
counterpart: **corpus verification** plus the binding imported by Go guests
(module `github.com/ybbh/mududb_p/bindings/go`). Pure stdlib,
TinyGo-compatible (no `unsafe`, no generics, no reflection). Layout:

- `codec/mpack.go` — hand-written canonical MessagePack writer / lenient
  reader over a native value model (`nil`/`bool`/integers/`float64`/
  `string`/`[]byte`/`[]any`/`map[uint64]any`), byte-exact with rmp_serde
  (minimal-width integers, str8 for 32..=255, map keys ascending).
- `codec/frame.go` — hand-written MSSP v1 16-byte header + request/result
  body helpers.
- `types/` — the mgen Go backend output (`-l go --with-func-codec`, 23
  `Uni*.go` files) plus the hand-written `wire.go` checked conversions
  (the only `types/` file without a "Generated" header).
- `corpus/` — `go test` suite replaying the three golden corpora.

Regenerate (from `crates/tools`, then copy `<scratch dir>/Uni*.go` into
`types/`):

```sh
cargo run -p mudu_gen -- message \
    -i ../common/mudu_binding/wit \
    -o <scratch dir> \
    -l go --with-func-codec
```

Test commands (in `crates/sdk/bindings/go`):

```sh
go build ./... && go vet ./... && test -z "$(gofmt -l .)"
go test ./...   # 44 primitive vectors (132 checks), 47 MSSP frames (188 checks), 8 lenient frames (16 checks)
```

### Adding a new language

Checklist for a new guest-language implementation (the wire side — for the
toolchain side: `mpm-crate` template, `mtp` front-end, `mgen` backends,
golden corpus and e2e wiring, see
[`dev/new_guest_language.md`](../../dev/new_guest_language.md)):

1. **Frame header.** 16 bytes, all big-endian: magic `0x4D535350`,
   version `1`, flags `0`, message kind `u32`. Reject (on decode) bad magic,
   any version other than `1`, non-zero flags, and unknown/`0` kinds.
2. **Request bodies.** MessagePack maps with the 1-based WIT parameter
   number as key (declaration order). Records are MessagePack maps
   keyed by the 1-based field numbers. Decoding is lenient: unknown map keys
   are skipped, missing fields default, and any integer width is accepted;
   structure (map shapes, tuple lengths, unknown variant tags) stays strict.
   Variants are `[tag, payload]`; `option<T>` is nil or value.
3. **Result bodies.** `[0u8, value]` / `[1u8, UniError]`.
4. **Blobs.** `list<u8>` as MessagePack bin — except `UniError.err_details`
   and `UniResultSet.cursor`, which are integer **arrays** (record context).
5. **Integer compaction.** Emit and accept the shortest MessagePack integer
   form (`rmp_serde` semantics): fixints for small values (`-2` → `0xFE`),
   bin8 `0xC4` for short blobs.
6. **Strict decode.** Reject trailing bytes after the body, malformed
   MessagePack, unknown variant tags, record/request bodies that are not
   maps, narrowing overflow, and invalid UTF-8 in strings. (Inside a map the
   decoder stays lenient: unknown keys skipped, missing fields defaulted,
   any integer width accepted.)
7. **Error surface.** Map `UniError` faithfully, including `err_src` as the
   JSON string `"None"` for source-less host errors and the fs errno
   mapping (`50029` → `EINVAL 22`).
8. **Verify.** Consume the two JSON sidecars before claiming conformance:
   `syscall_payload_v1_all.json` (47-frame semantic expectations — decode →
   field-level semantic assertions → re-encode → decode again for semantic
   equality; request frames additionally re-encode byte-exactly, pinning the
   canonical encoder) and `lenient_decode_v1.json` (8 lenient-decode
   vectors — replay the non-canonical byte shapes against the sidecar's
   `expect` values). See
   [Interop verification](#interop-verification) and
   [Lenient decode vectors](#lenient-decode-vectors).

## Interop verification

All implementations are pinned to a single host-generated golden corpus:
[`testing/fixtures/golden/v1/syscall_payload_v1_all.bin`](../../../crates/db-kernel/testing/fixtures/golden/v1/syscall_payload_v1_all.bin)
plus its JSON sidecar `syscall_payload_v1_all.json`.

- **Contents:** 47 frames — kinds 1–23, each as request + ok-response in
  discriminant order, plus one `UniError` get-response.
- **Container format:** concatenated big-endian `u32` length-prefixed MSSP
  frames.
- **Assertion style:** the corpus is pinned by **semantic assertions +
  roundtrips**, not by a whole-file byte anchor — per frame: decode →
  field-level semantic assertions → re-encode → decode → semantic equality;
  request frames additionally re-encode byte-exactly (still pinning the
  canonical encoder); the `UniError` frame stays field-level (a decoded
  error carries a fresh caller location and cannot re-encode byte-exactly).
- **Sidecar:** `syscall_payload_v1_all.json` follows the same shape as the
  mp_primitives sidecar (top-level format/reference/container/regenerate
  fields); its `frames` array lists `(index, message_kind,
  message_kind_name, direction, expect)` per segment, where `expect`
  describes the decoded key fields as JSON (u64/i64/u128 as decimal strings,
  byte strings as lowercase hex, a UniOid as `{h, l}` decimal strings, unit
  results as `{"unit": true}`) so the C# / AssemblyScript / Python consumers
  can rebuild every expectation without a Rust toolchain. Frame bytes and
  sidecar expectations derive from the same constructors
  (`golden_syscall_all_cases`); the cross-check test
  `syscall_all_sidecar_matches_bin` keeps them from drifting.
- **Verified by seven independent codecs:**

  | Implementation | Test |
  |----------------|------|
  | Host router | `testing/tests/compat_golden.rs::golden_v1_all_kinds_roundtrip` |
  | Standalone Rust SDK | `mudu_api/rust/tests/golden_frames_test.rs` |
  | C# SDK | `Mudu.Api.Tests/GoldenFrameCorpusTests.cs` |
  | AssemblyScript generated codec | `bindings/assemblyscript/run_syscall_corpus_test.mjs` (172 checks) |
  | Python generated codec | `bindings/python/run_syscall_corpus_test.py` (188 checks) |
  | C generated codec | `bindings/c/tests/corpus_driver.c` (native; 398 checks across the three suites) |
  | Go generated codec | `bindings/go/corpus/syscall_corpus_test.go` (188 checks) |

- The **host codec is authoritative**; in any discrepancy the host wins.
- Regeneration (host side, writes both the `.bin` and the `.json` fixture
  files):

  ```sh
  cargo test -p testing --test compat_golden -- generate_golden_v1_fixtures --ignored
  ```

Zero discrepancies have been found across the seven implementations.

### Lenient decode vectors

The corpus directory also carries a fixture pair that pins the **lenient**
side of the MSSP v1 decoder: `lenient_decode_v1.bin` plus its sidecar
`lenient_decode_v1.json`. Each vector is a hand-built, legal but
**non-canonical** MSSP frame (byte shapes the canonical encoder can never
emit) together with its expected decoded semantics:

| kind | Contents |
|------|----------|
| `int-width-widening` | u64 value 2 encoded with the u32 marker `0xCE` |
| `int-unsigned-marker` | non-negative i64 encoded with the unsigned u8 marker `0xCC` |
| `int-wide-negative` | small negative -2 encoded with the i64 marker `0xD3` |
| `record-key-order` | record map keys out of order (4,3,2,1) |
| `record-unknown-field` | record with an unknown field-number key (skipped) |
| `map-noninteger-key` | map with a string key (skipped) |
| `request-missing-param` | request map missing a parameter (proto3 default) |
| `record-missing-fields` | record missing fields (defaults, incl. a variant field's default case) |

The sidecar lists `(index, kind, message_kind, message_kind_name, direction,
note, expect)` per segment; Rust verifies every vector in
`compat_golden.rs::lenient_decode_v1_vectors`, and the C# / AS / Python /
C / Go consumers replay the same leniency rules against the sidecar's
`expect` values (on the Python side via
`bindings/python/run_lenient_decode_test.py`, 16 checks; C and Go replay
theirs in `bindings/c/tests/corpus_driver.c` and
`bindings/go/corpus/lenient_decode_test.go`). The pair regenerates with the
same command as the canonical corpus above.

## MP primitive alignment

The frame-level corpus above is complemented by a primitive-level corpus
that pins the rmp_serde 1.3.1 encoding rules every guest MessagePack runtime
must reproduce byte for byte:
[`testing/fixtures/golden/v1/mp_primitives_v1.bin`](../../../crates/db-kernel/testing/fixtures/golden/v1/mp_primitives_v1.bin)
plus its JSON sidecar `mp_primitives_v1.json`.

- **Contents:** 44 single-value vectors — u64 boundaries (0, 1, 127, 128,
  255, 256, 65535, 65536, 2^32-1, 2^32, 2^64-1), i64 boundaries (1, 127,
  128, -1, -32, -33, -128, -129, -32768, -32769, -2^31, -2^31-1, i64::MIN),
  f32 (1.5, 0.1), f64 (1.5, -π), nil, both bools, strings (lengths
  0/31/32/255/256 plus the multibyte `héllo世界`), bins (lengths 0/255/256),
  arrays (lengths 0/15/16), and one nested `[u64, str, bin]` combo.
- **Container format:** the same big-endian-u32 length-prefixed segments as
  the syscall corpus; the sidecar lists `(index, kind, value|len)` per
  segment so non-Rust consumers can rebuild each expected value.
- **Assertion style:** the primitive encoding rules are unchanged, so this
  corpus keeps its **whole-file byte anchor** — `mp_primitives_v1_roundtrip`
  asserts the committed `.bin` matches the current rmp_serde encoder byte
  for byte (the sidecar text is pinned byte-exactly too), unlike the
  semantic + roundtrip style of the syscall corpus.
- **Sidecar inventory:** the corpus directory now holds three JSON sidecars
  — `mp_primitives_v1.json` (primitive vectors),
  `syscall_payload_v1_all.json` (47-frame semantic expectations), and
  `lenient_decode_v1.json` (lenient decode vectors) — all host-generated
  and shared across the five languages.

### Pinned encoding rules

| Value | Encoding |
|-------|----------|
| u64 | minimal width **by value**: fixint ≤ 127, `0xCC` ≤ 255, `0xCD` ≤ 65535, `0xCE` ≤ 2^32-1, `0xCF` above |
| i64 ≥ 0 | the **unsigned** marker chain (`128i64` → `0xCC 0x80`) |
| i64 < 0 | negfixint -1..-32, then `0xD0` / `0xD1` / `0xD2` / `0xD3` |
| f32 / f64 | `0xCA` / `0xCB`, big-endian IEEE-754 |
| str | fixstr ≤ 31, str8 `0xD9` 32..=255, str16 `0xDA` 256..=65535, str32 `0xDB` above — rmp_serde **does** emit str8 |
| bin | bin8 `0xC4` ≤ 255, bin16 `0xC5`, bin32 `0xC6` |
| array | fixarray ≤ 15, array16 `0xDC` ≤ 65535, array32 `0xDD` above |

Integer width is by **value**, not by source type: rmp_serde encodes `2u8`,
`2u16`, `2u32` and `2u64` all as the single fixint `0x02`.

### Decode leniency and rejection rules

- f32/f64 decode **leniently** across the `0xCA`/`0xCB` markers in both
  directions (`from_slice::<f32>` accepts an f64 encoding and vice versa).
- Reading a u64 **rejects** negative encodings (`from_slice::<u64>` on a
  negfixint errors).
- Gotcha: a Rust newtype struct wrapping a blob (`struct W(Vec<u8>)`)
  serializes **transparently** in rmp_serde — no array wrapper. Newtype
  wrappers are therefore deliberately absent from the primitive corpus;
  their wire shape is pinned by the syscall corpus instead.

### Primitive corpus verification

- **Verified byte-exact by six independent runtimes:**

  | Implementation | Test |
  |----------------|------|
  | Host (rmp_serde 1.3.1) | `testing/tests/compat_golden.rs::mp_primitives_v1_roundtrip` |
  | C# (MessagePack-CSharp 3.1.4) | `Mudu.Api.Tests/MpPrimitiveCorpusTests.cs` |
  | AssemblyScript (`mpack.ts`) | `bindings/assemblyscript/run_mp_corpus_test.mjs` |
  | Python (`mududb/codec/mpack.py`) | `bindings/python/run_mp_corpus_test.py` |
  | C (`mududb/codec/mpack.c`) | `bindings/c/tests/corpus_driver.c` |
  | Go (`codec/mpack.go`) | `bindings/go/corpus/mp_corpus_test.go` |

- **rmp_serde is authoritative**; in any discrepancy the corpus wins and the
  guest runtime is fixed, never the fixture.
- Regeneration (host side, writes both fixture files):

  ```sh
  cargo test -p testing --test compat_golden -- generate_mp_primitives_v1 --ignored
  ```

- Per-language verification:

  ```sh
  cargo test -p testing --test compat_golden   # Rust, in crates/db-kernel
  dotnet test Mudu.Api.Tests                   # C#, in crates/sdk/mudu_api/csharp
  node run_mp_corpus_test.mjs                  # AS, in crates/sdk/bindings/assemblyscript
  python3 run_mp_corpus_test.py                # Python, in crates/sdk/bindings/python
  make check                                   # C, in crates/sdk/bindings/c
  go test ./...                                # Go, in crates/sdk/bindings/go
  ```

Zero discrepancies have been found across the six runtimes:
MessagePack-CSharp's integer writes are minimal-width by value and it emits
str8, matching rmp_serde on all 44 vectors.

## Version policy

- The runtime decodes **only the current version** (`1`). Anything else
  fails fast with `ErrorCode::UnsupportedFormatVersion`.
- Every MPK package pins the same version: `package.manifest.json` carries
  `syscall_abi_version: u32`. `mpm_build` always emits
  `SYSCALL_PAYLOAD_CURRENT_VERSION` (`1`); the loader
  (`mudu_runtime/src/service/app_package.rs`) defaults a missing field to
  `1` and rejects any mismatch with `ErrorCode::UnsupportedFormatVersion`
  ("unsupported syscall ABI version {found}, expected {expected}").
  `FormatKind::SyscallPayload` is registered in the global
  `CompatibilityRouter` (`mudu_kernel/src/compat.rs`) with an identity v1
  migration handler (`mudu_binding::codec::syscall_payload::migrate`);
  migrate units are complete frames.
- There is **no legacy MPK compatibility**: packages are rebuilt per ABI
  version. The version field exists for forward control of future upgrades,
  not for cross-version interop.
- Deprecation of v1 requires all guest bindings (Rust, C#, AssemblyScript,
  Python) to emit and accept the successor version first; see the
  [contract document](../contract/syscall_payload_v1.md) for the full
  upgrade and deprecation rules.

## References

- Contract: [`doc/en/contract/syscall_payload_v1.md`](../contract/syscall_payload_v1.md)
- WIT syscall interface: [`uni-syscall.wit`](../../../crates/common/mudu_binding/wit/uni-syscall.wit)
- Runtime world: [`mudu_runtime/wit/api.wit`](../../../crates/db-kernel/mudu_runtime/wit/api.wit)
- Host codec: [`mudu_binding/src/codec/syscall_payload/`](../../../crates/common/mudu_binding/src/codec/syscall_payload/)
  (regen: [`REGENERATE.md`](../../../crates/common/mudu_binding/src/codec/syscall_payload/REGENERATE.md))
- Compat registry: [`mudu/src/compat/mod.rs`](../../../crates/common/mudu/src/compat/mod.rs)
- Standalone Rust SDK: [`crates/sdk/mudu_api/rust/`](../../../crates/sdk/mudu_api/rust/)
  (regen: crate-root `REGENERATE.md`)
- C# SDK: [`crates/sdk/mudu_api/csharp/`](../../../crates/sdk/mudu_api/csharp/)
  (regen: crate `README.md`)
- AssemblyScript generated codec: [`crates/sdk/bindings/assemblyscript/assembly/generated/`](../../../crates/sdk/bindings/assemblyscript/assembly/generated/)
- Python generated codec (tooling / corpus verification): [`crates/sdk/bindings/python/`](../../../crates/sdk/bindings/python/)
- wallet-as direct byte-pipe build: [`crates/sdk/example/wallet-as/Makefile.toml`](../../../crates/sdk/example/wallet-as/Makefile.toml)
- wallet-cs (C# component guest): [`crates/sdk/example/wallet-cs/`](../../../crates/sdk/example/wallet-cs/)
- Golden corpus: [`testing/fixtures/golden/v1/syscall_payload_v1_all.bin`](../../../crates/db-kernel/testing/fixtures/golden/v1/syscall_payload_v1_all.bin)
- MP primitive corpus: [`testing/fixtures/golden/v1/mp_primitives_v1.bin`](../../../crates/db-kernel/testing/fixtures/golden/v1/mp_primitives_v1.bin) (+ `mp_primitives_v1.json` sidecar)
