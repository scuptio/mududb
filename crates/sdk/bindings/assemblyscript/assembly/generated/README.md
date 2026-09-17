# Generated AssemblyScript MSSP codec — do not edit manually

The files in this directory are produced by `mudu_gen` (`mgen`) from the WIT
contracts in `crates/common/mudu_binding/wit`, the same source of truth as the
host-side Rust codec (see
`crates/common/mudu_binding/src/codec/syscall_payload/REGENERATE.md`). They
are what lets an AssemblyScript guest talk to the host directly: the guest
imports the byte-pipe `mududb:api/system` interface and frames MSSP itself
with this codec (see `assembly/syscall.ts`), with no shim component involved.

## Command

From `crates/tools`:

```sh
cargo run -p mudu_gen -- message \
    -i ../common/mudu_binding/wit \
    -o <scratch dir> \
    -l assemblyscript --with-func-codec
```

## Where the output lands

Copy all `<scratch dir>/*.ts` files into this directory
(`crates/sdk/bindings/assemblyscript/assembly/generated/`), then rewrite the
runtime import in every file, because the mgen template emits `./mpack` while
the runtime lives one level up:

```sh
sed -i 's|from "./mpack"|from "../mpack"|' assembly/generated/*.ts
```

All other generated imports (`./UniXxx`) stay inside this directory and need
no rewriting.

## WIT pinning notes

Same pins as the host codec (see the Rust `REGENERATE.md`):

- `uni-syscall.wit` declaration order **is** the wire `message_kind` numbering
  (1–23). Never reorder it.
- `uni-scalar.wit` discriminants are the deployed wire values: `u128` follows
  `u64`, `i128` follows `i64`, and the last case is `timestamp-tz`
  (PascalCase `TimestampTz`).
- `uni-data-type.wit` ends with `%box(box<uni-data-type>)` (wire tag 8).

AssemblyScript-specific shape decisions (made by the mgen AS templates):

- Records encode as MessagePack **maps keyed by the 1-based field number**
  (WIT declaration order); request bodies are likewise maps keyed by the
  1-based parameter number, and a 0-parameter request body is empty (no map
  header). Decode is lenient (proto3 semantics): unknown keys are skipped
  (`MpackReader.skipValue`), keys may arrive in any order, and missing
  fields/parameters keep their type default. Non-integer or negative map keys
  are consumed and skipped via `MpackReader.readMapKey` (reported as the
  never-used key 0).
- Result envelopes stay `[0u8, value]` / `[1u8, UniError]` arrays; WIT
  variants stay `[tag, payload]` arrays with the `0u8` placeholder for
  payload-less cases, and unknown tags are rejected.
- WIT variants become a base class carrying a `kind` tag with one subclass
  per case (AS has no union types); codecs downcast with checked `as` casts.
- Per-func request/response tuples become holder classes
  (`GetRequest`, `GetResult`, ...); results carry `value` + `error` with
  `ok`/`err` static constructors.
- `u128`/`i128` scalars are `Uint8Array` (AS has no 128-bit integers) and
  encode as MessagePack arrays of `u64`, matching the host's `Vec<u8>` serde
  form.
- The pinned host quirks are reproduced: `UniError.err_details` and
  `UniResultSet.cursor` encode as MessagePack ARRAYs of `u64`, not bin;
  `UniDataValue.Binary` likewise encodes as an array (plain `list<u8>`, no
  serde-bytes annotation on the host side). Only func-level `list<u8>`/`blob`
  parameters and results use MessagePack bin.

## After regenerating

Run, from `crates/sdk/bindings/assemblyscript`:

```sh
node run_mpack_test.mjs
node run_mp_corpus_test.mjs
node run_syscall_corpus_test.mjs
```

`run_syscall_corpus_test.mjs` is the byte-exact pin: it checks every frame of
`crates/db-kernel/testing/fixtures/golden/v1/syscall_payload_v1_all.bin`
(47 frames) against the generated codec. If regenerated code changes any
frame byte, stop and reconcile the WIT or the templates instead of updating
the fixtures.
