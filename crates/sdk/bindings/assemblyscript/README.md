# MuduDB AssemblyScript Binding

This package is the AssemblyScript guest binding: guests import the host's
`mududb:api/system` byte-pipe interface directly (23 `func(list<u8>) ->
list<u8>` syscalls) and frame MSSP (SyscallPayload v1) in-language with the
mgen-generated, corpus-verified codec in `assembly/generated/` — the same
direct byte-pipe architecture as the C# guest
(`crates/sdk/example/wallet-cs`). No companion Rust component is involved:

```text
AssemblyScript guest component
  imports mududb:api/system        (syscall.ts @external declarations)
  exports mp2-<proc> byte pipes    (mtp-generated adapter)

host runtime
  satisfies mududb:api/system, calls the root-level mp2-* exports
```

## Install and imports

```sh
npm install @mududb/mududb
```

The package ships pure AssemblyScript source; consumers compile it with
their own `asc`. This package is the **reference implementation** of the
cross-language canonical facade surface — see
[`doc/dev/binding_api_surface.md`](../../../doc/dev/binding_api_surface.md)
for the operation-level contract shared with the C#, Rust, and (pending)
Python bindings. The root entry re-exports the full public surface:

```ts
import { Database, SqlStmt, ValueList, Value, Oid, Result } from "@mududb/mududb";
```

Canonical subpath entries mirror the sibling bindings' module paths
(`mududb.types`, `mududb.codec`, ...):

```ts
import { Oid, Value, ValueKind, MuduError, UniOid } from "@mududb/mududb/types";
import { MpackWriter, MessageKind, encodeFrame } from "@mududb/mududb/codec";
import { Database } from "@mududb/mududb/db";
import { SqlStmt, ValueList } from "@mududb/mududb/sql";
import { fsOpen, FS_O_RDONLY } from "@mududb/mududb/fs";
import { Result, ResultSet } from "@mududb/mududb/result";
import { witOpen } from "@mududb/mududb/sys";
import { UniQueryArgv } from "@mududb/mududb/generated";
```

`asc` 0.27 resolves packages by plain file path (it ignores package.json
`exports`/`main`), so each subpath is a real file at the package root —
`@mududb/mududb/types` resolves to `<pkg>/types.ts`, which re-exports
`./assembly/types`. `cabi_realloc` is exported only from the root entry,
for the mtp-generated adapter to re-export.

## Layout

```text
assembly/
  wit.ts       Core DX types (Oid, MuduError, Value) and their uni wire
               conversions, plus alloc/utf8 helpers and cabi_realloc.
  syscall.ts   MSSP client: mududb:api/system byte-pipe imports plus the
               witOpen/witClose/witQuery/witCommand/witBatch functional API
               and the fs syscall wrappers.
  sql.ts       Pure-AS SqlStmt and ValueList (positional/named binding).
  result.ts    Result<T>, in-memory ResultSet/Row, procedureResultOk/Err.
  database.ts  Database facade: open / close / query / command / batch.
  fs.ts        FsStat / FsDirEntry types and the fsOpen ... fsReaddir wrappers.
  procedure.ts mp2 byte-pipe helpers for the mtp adapter (decode
               UniProcedureParam, encode UniResult<UniProcedureResult,
               UniError>).
  record.ts    Record bridge for user-defined (mgen-generated) procedure
               types: transcodes the uni-data-value record-case envelope to
               the integer-keyed MessagePack map the generated XxxCodec
               classes consume/produce (recordFieldValues /
               recordFromFieldValues / isNullDatum).
  mpack.ts     MessagePack runtime (byte-exact with rmp-serde).
  generated/   mgen-generated MSSP/uni codec (do not edit; see its README).
  index.ts     Public exports (root entry).
  types.ts     Canonical mududb.types entry (wit DX types + all Uni* types).
  codec.ts     Canonical mududb.codec entry (mpack + procedure + MSSP frame
               codec).
  db.ts        Canonical mududb.db entry (Database).
  sys.ts       Canonical mududb.sys entry (syscall wrappers).

index.ts, types.ts, codec.ts, db.ts, sql.ts, fs.ts, result.ts, sys.ts,
generated.ts (package root)
               Thin re-exports giving asc's plain file-path resolution the
               `@mududb/mududb` root and every `@mududb/mududb/<subpath>`
               import.

example/
  assembly/    Smoke example (SQL + fs roundtrip).
```

## Filesystem (fs) API

`assembly/fs.ts` wraps the 11 synchronous `fs-*` syscalls of
`mududb:api/system`:

- `fsOpen(sessionHi, sessionLo, oidHi, oidLo, path, flags): Result<u32>`
- `fsClose(sessionHi, sessionLo, fd): Result<bool>`
- `fsRead(sessionHi, sessionLo, fd, len): Result<ArrayBuffer>`
- `fsWrite(sessionHi, sessionLo, fd, data): Result<u32>`
- `fsPread(sessionHi, sessionLo, fd, offset, len): Result<ArrayBuffer>`
- `fsPwrite(sessionHi, sessionLo, fd, offset, data): Result<bool>`
- `fsLseek(sessionHi, sessionLo, fd, offset, whence): Result<u64>`
- `fsFstat(sessionHi, sessionLo, fd): Result<FsStat>`
- `fsStat(sessionHi, sessionLo, oidHi, oidLo, path): Result<FsStat>`
- `fsFsync(sessionHi, sessionLo, fd): Result<bool>`
- `fsReaddir(sessionHi, sessionLo, oidHi, oidLo, path): Result<FsDirEntry[]>`

The session id comes from `Database.open(...).id`. `flags` are the libc
`O_*` access-mode bits (`FS_O_RDONLY` / `FS_O_WRONLY` / `FS_O_RDWR`) and
`whence` is `FS_SEEK_SET` / `FS_SEEK_CUR` / `FS_SEEK_END`.

```ts
const db = Database.open("");
const s = db.id;
const fd = fsOpen(s.hi, s.lo, oidHi, oidLo, "hello.txt", FS_O_WRONLY).unwrap();
fsWrite(s.hi, s.lo, fd, String.UTF8.encode("hello fs", false)).unwrap();
fsClose(s.hi, s.lo, fd).unwrap();
```

The smoke example shows a full write->read roundtrip (`fsSmoke` in
`example/assembly/index.ts`). Only the synchronous fs surface is bound;
async fs awaits AssemblyScript component-model async ABI support.

## SQL parameters

`assembly/sql.ts` `ValueList` carries the bind values for one statement:

- `bind(index, value)` — positional `?` placeholders. Indices must be
  contiguous from 0; gaps, negatives, and re-binding the same index are
  rejected.
- `bindNamed(name, value)` — named `:name` placeholders. Named parameters
  are resolved **host-side**: the statement text keeps the `:name`
  placeholders, the names travel in the `param-names` wire field, and the
  host rewrites them to positional form, matching values by name (a
  repeated `:name` reuses its value). Indexed and named values cannot be
  mixed in one list.

SQL NULL binds through `Value.null()` and round-trips in both
directions (params guest→host, results host→guest).

## Build Core Wasm

```sh
npm install
npm run build
```

Compile the smoke example:

```sh
npx asc example/assembly/index.ts --outFile build/release/example.wasm --optimize
```

For a full guest package (mtp adapter generation, component embed/new, mpk
packaging) see `crates/sdk/example/wallet-as`.

## Codec verification

```sh
node run_mpack_test.mjs
node run_mp_corpus_test.mjs
node run_syscall_corpus_test.mjs
node run_lenient_decode_test.mjs
node run_record_bridge_test.mjs
```

`run_syscall_corpus_test.mjs` is the byte-exact pin: every frame of the
host-generated golden corpus is checked against the generated AS codec.
