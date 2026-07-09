# Project-Controlled Guest→Host Syscall ABI

## Executive Summary

Replace serde/MessagePack as the authoritative wire format for MuduDB runtime syscalls with a project-owned, versioned binary format. The new format is based on the existing `mudu_binding::universal` schema, which is now fully expressed as canonical WIT files under `mudu_binding/wit/`. The MessagePack wire layout is generated from these WIT files by `mudu_gen` for all guest languages (Rust, C#, AssemblyScript) using project-controlled templates, so the project is no longer coupled to wit-bindgen for type marshalling or to the default `rmp_serde` derive behavior for payload encoding.

The WIT boundary remains a thin opaque byte interface (`list<u8>`). Guest code calls the WIT functions declared in `uni-syscall.wit`; the generated bindings serialize the function arguments into the 16-byte header + project-controlled MessagePack body, and deserialize the response.

When the syscall ABI version changes, MPK packages are rebuilt for the matching runtime. The runtime does not retain legacy MPK compatibility; versioning is used only to make future upgrades explicit and controllable.

---

## 1. Current Architecture

```text
┌─────────────────────────────────────────────────────────────────────────┐
│                              Guest (WASM component)                     │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────────────────────┐   │
│  │ Rust guest   │  │ C# guest     │  │ AssemblyScript guest          │  │
│  │ rmp_serde    │  │ MessagePack  │  │ rs-shim → rmp_serde           │  │
│  └──────┬───────┘  └──────┬───────┘  └──────────────┬────────────────┘  │
│         │                 │                          │                   │
│         └─────────────────┴──────────────────────────┘                   │
│                              │                                          │
│                         WIT import (list<u8>)                            │
└──────────────────────────────┼──────────────────────────────────────────┘
                               │
┌──────────────────────────────┼──────────────────────────────────────────┐
│                         Host │ runtime                                   │
│  ┌───────────────────────────┘                                          │
│  │ mudu_runtime/src/interface/kernel.rs                                  │
│  │ mudu_binding::codec::handle_sys_*                                     │
│  │ rmp_serde decode → kernel calls → rmp_serde encode                    │
│  └───────────────────────────────────────────────────────────────────────┘
```

### Where serde/MessagePack is used today

| Layer | File | Mechanism |
|-------|------|-----------|
| SQL request serialization (host) | `mudu_binding/src/codec/handle_sys_incoming.rs` | `rmp_serde::encode::to_vec` of `UniQueryArgv` / `UniCommandArgv` |
| SQL response serialization (host) | `mudu_binding/src/codec/handle_sys_outcoming.rs` | `rmp_serde` of `UniResult<UniQueryResult, UniError>` |
| SQL request/response (Rust guest) | `mudu_api/rust/src/mudu_sys/mod.rs` | `rmp_serde::to_vec` / `from_slice` |
| SQL request/response (C# guest) | `mudu_api/csharp/uni/*.cs` | MessagePack for C# attributes and custom formatters |
| SQL request/response (AS guest) | `bindings/rs-shim/src/facade.rs` | `mudu_binding::system::{query,command}_invoke` → `rmp_serde` |
| KV syscalls | `mudu_binding/src/codec/handle_sys_session.rs` | **Already hand-written binary** |

The canonical schema is the set of WIT files under `mudu_binding/wit/`. `mgen` reads these files and emits language-specific source files using project-controlled templates:

| Language | Template location | Output |
|----------|-------------------|--------|
| Rust | `mudu_gen/templates/rust/` | Custom `serde` implementations in `mudu_binding/src/universal/` |
| C# | `mudu_gen/templates/csharp/` | Custom `IMessagePackFormatter<T>` implementations |
| AssemblyScript | `mudu_gen/templates/assemblyscript/` | Custom MessagePack encode/decode functions |

The generated MessagePack layout overrides default `rmp_serde` behavior:
- `record` → fixed-length MessagePack array.
- `variant` → two-element array `[u32 tag, payload]`.
- `enum` → bare `u32` discriminant.
- `list<T>` → MessagePack array.
- `option<T>` → nil for `none`, encoded `T` for `some`.
- `result<T, E>` → `[ok_tag, value]`.
- `string` → MessagePack str.
- `list<u8>` / `blob` → MessagePack bin.
- `box<T>` → transparent on the wire.

The `uni-syscall.wit` file declares the syscall functions (`query`, `command`, `batch`, `open-session`, `close-session`, `get`, `put`, `delete`, `range`). The runtime maps each WIT function call to the internal header + MessagePack payload; the `message_kind` field in the header is used only for runtime routing and is derived from the WIT function being called.

---

## 2. Target Architecture

```text
┌─────────────────────────────────────────────────────────────────────────┐
│                              Guest                                       │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────────────────────┐  │
│  │ Rust guest   │  │ C# guest     │  │ AssemblyScript guest          │  │
│  │ project      │  │ generated    │  │ generated or hand-written     │  │
│  │ codec        │  │ codec        │  │ codec                         │  │
│  └──────┬───────┘  └──────┬───────┘  └──────────────┬────────────────┘  │
│         │                 │                          │                   │
│         └─────────────────┴──────────────────────────┘                   │
│                              │                                          │
│                         WIT import (list<u8>) — unchanged               │
└──────────────────────────────┼──────────────────────────────────────────┘
                               │
┌──────────────────────────────┼──────────────────────────────────────────┐
│                         Host │ runtime                                   │
│  ┌───────────────────────────┘                                          │
│  │ mudu_runtime/src/interface/kernel.rs                                  │
│  │ mudu_binding::codec::syscall_payload                                  │
│  │ project codec decode → kernel calls → project codec encode            │
│  └───────────────────────────────────────────────────────────────────────┘
```

Key properties:
- **Schema** = canonical WIT files under `mudu_binding/wit/`.
- **Wire format** = 16-byte header + project-controlled MessagePack body, versioned via `mudu::compat`.
- **Language bindings** = generated from WIT by `mudu_gen`; Rust uses the generated code as the reference implementation.
- **WIT boundary** = still opaque bytes; no wit-bindgen type marshalling.
- **MPK compatibility** = no legacy support; packages are rebuilt per ABI version.

---

## 3. Format Design: `SyscallPayload`

### 3.1 Registry Entry

Add to `mudu/src/compat/mod.rs`:

```rust
pub enum FormatKind {
    // ... existing variants ...
    /// Guest→host syscall payload format.
    SyscallPayload,
}

pub const SYSCALL_PAYLOAD_CURRENT_VERSION: u32 = 1;

// Magic: 0x4D53_5350 "MSSP"
```

### 3.2 Payload Layout (v1)

Every syscall request and response is encoded as:

```text
+--------------------------------+
| Header (16 bytes)              |
+--------------------------------+
| Body (variable)                |
+--------------------------------+
```

**Header — big-endian**

| Offset | Size | Field | Value |
|--------|------|-------|-------|
| 0 | 4 | `magic` | `0x4D53_5350` |
| 4 | 4 | `version` | `1` |
| 8 | 4 | `flags` | Reserved, must be `0` |
| 12 | 4 | `message_kind` | Discriminant: `Query`, `Command`, `Batch`, `Open`, `Close`, `Get`, `Put`, `Delete`, `Range` |

**Body — compact tagged encoding**

- Fixed-width scalars are written in big-endian.
- Length-prefixed blobs use a 4-byte big-endian length.
- Variants use a 1-byte tag.
- OID is always 16 bytes (u128).
- `Result<T, E>` uses a 1-byte tag (`0x00` Ok, `0x01` Err) followed by the payload.

This matches the style already proven in `handle_sys_session.rs` and avoids the variable-length overhead of MessagePack varints for hot scalar fields.

### 3.3 Message Kinds and Body Types

| `message_kind` | WIT function | Request Body | Response Body |
|----------------|--------------|--------------|---------------|
| `Query` | `query` | `UniQueryArgv` | `Result<UniQueryResult, UniError>` |
| `Command` | `command` | `UniCommandArgv` | `Result<UniCommandResult, UniError>` |
| `Batch` | `batch` | `UniCommandArgv` (with batched params) | `Result<UniCommandResult, UniError>` |
| `Open` | `open-session` | `UniSessionOpenArgv` | `Result<UniOid, UniError>` |
| `Close` | `close-session` | `UniOid` | `Result<(), UniError>` |
| `Get` | `get` | `(UniOid, key: Vec<u8>)` | `Result<Option<Vec<u8>>, UniError>` |
| `Put` | `put` | `(UniOid, key: Vec<u8>, value: Vec<u8>)` | `Result<(), UniError>` |
| `Delete` | `delete` | `(UniOid, key: Vec<u8>)` | `Result<(), UniError>` |
| `Range` | `range` | `(UniOid, start: Vec<u8>, end: Vec<u8>)` | `Result<Vec<(key, value)>, UniError>` |

The mapping from WIT function to `message_kind` is an internal runtime detail; guest code should call the WIT functions directly.

### 3.4 Version Semantics

- `version` in the header is the **payload format version**, not the WIT interface version.
- The runtime decodes only the current version. Unsupported versions fail fast with `ErrorCode::UnsupportedFormatVersion`.
- When a future v2 is introduced, offline migration handlers convert stored fixtures/test data; runtime does **not** keep legacy decoders.

---

## 4. Performance Guarantee

### 4.1 Can the Custom Encoder Beat `rmp_serde`?

**Yes — if designed correctly.** The custom encoder has structural advantages that `rmp_serde` cannot exploit because it is a general-purpose serializer:

1. **No serde trait machinery.** `rmp_serde` routes every field through the `Serialize` trait and a state-machine serializer. A hand-written codec writes known fields directly, eliminating virtual calls and trait dispatch.
2. **Known schema = known layout.** The encoder knows the exact order and types of `UniQueryArgv`, `UniDataValue`, `UniScalarValue`, etc. It can unroll loops, inline hot paths, and avoid runtime type introspection.
3. **Optimal scalar widths.** MessagePack uses variable-length integers. For syscall payloads, most integers are OID (u128), session IDs (u128), row counts (u64), and small enum tags. A custom format can use fixed-width fields for these, removing varint decode overhead.
4. **No map/array framing overhead.** MessagePack prefixes every array/map with a format marker and length. A schema-driven format embeds lengths only where necessary (variable-length blobs), cutting metadata bytes.
5. **Single allocation.** With a two-pass encode (size pass + write pass) or exponential buffer growth like `rmp_serde`, the encoder allocates once and writes contiguously.
6. **Cache-friendly ordering.** Fields are laid out in the order they are consumed by the host, improving decode cache locality.
7. **Specialized string handling.** SQL statements are passed as UTF-8 bytes; the custom encoder can copy them verbatim instead of going through MessagePack string encoding rules.

### 4.2 Where Will the Biggest Wins Come From?

| Workload | Why custom wins |
|----------|-----------------|
| Large `UniQueryResult` with many rows | Fixed-width row layout, no per-column map markers, direct tuple decoding. |
| Many small KV ops | Fixed 16-byte OID + 4-byte length + raw bytes; no varints, no map headers. |
| Nested `UniDataValue` arrays/records | 1-byte variant tag + inline payload instead of MessagePack array wrapper. |
| Error responses | Tiny fixed header + compact `UniError` instead of a full MessagePack map. |

### 4.3 Acceptance Criteria

Before the old `rmp_serde` path is removed, the new encoder must meet:

| Metric | Target |
|--------|--------|
| Small request payload (≤ 256 bytes) | Encode/decode within **±10%** of `rmp_serde` wall time. |
| Large result set (≥ 10k rows) | Encode/decode **≥ 20% faster** than `rmp_serde`. |
| Allocations per encode | **≤ 1** heap allocation (the output buffer). |
| Allocations per decode | **0** for in-place decode of scalar fields; 1 for the result container. |
| Payload size | **≤ 1.1×** the `rmp_serde` payload size for representative workloads. |

If the first implementation misses these targets, the format or implementation is optimized before the migration is declared complete.

### 4.4 Measurement Plan

1. Add a criterion benchmark in `mudu_binding/benches/syscall_payload_bench.rs`.
2. Cover representative payloads:
    - `UniQueryArgv` with a short SQL statement and 10 parameters.
    - `UniQueryArgv` with a long SQL statement and 100 parameters.
    - `UniQueryResult` with 1, 100, and 10,000 rows of mixed scalar types.
    - KV `Get`/`Put`/`Range` requests and responses.
    - Error responses.
3. Benchmark both encode and decode, measuring wall time, allocations (via `dhat` or `stats_alloc`), and output size.
4. Compare against the current `rmp_serde` path using the same `universal` types.
5. Document results in `doc/en/contract/syscall_payload_v1.md`.

---

## 5. Why a Hand-Written Encoder Can Be Slower (and How to Prevent It)

`rmp_serde` is mature and optimized. A naive custom encoder can lose if it:

- Allocates a temporary `Vec` for every field or variant.
- Copies sub-buffers instead of writing directly into the output.
- Uses fixed-width fields without a sizing pass, causing multiple reallocations.
- Implements generic recursion inefficiently for nested `UniDataValue` arrays/records.
- Uses `Result`-heavy error paths on every byte read instead of fast assertions.

Prevention rules for the reference implementation:

1. **Pre-compute sizes** and allocate the output buffer once, or grow a single `Vec` exponentially.
2. **Write directly** into the final buffer with a cursor; no intermediate buffers.
3. **Use fixed-width scalars** for hot fields (OID, lengths, row counts) and length-prefix only variable blobs.
4. **Avoid `Vec` reallocation** by reserving capacity based on the sizing pass.
5. **Benchmark early** and optimize the hot path (large result sets) before declaring done.

---

## 6. Implementation Phases

### Phase 1 — WIT Schema, Format Contract and Registry

| Task | Owner | Output |
|------|-------|--------|
| Stabilize `mudu_binding/wit/*.wit` as canonical schema | Design | WIT files for `uni-data-type`, `uni-data-value`, `uni-scalar`, `uni-scalar-value`, `uni-record-type`, `uni-result-type`, `uni-error`, `uni-oid`, `uni-syscall`, etc. |
| Add `FormatKind::SyscallPayload` and version constants | Rust | `mudu/src/compat/mod.rs` updated |
| Write English contract | Docs | `doc/en/contract/syscall_payload_v1.md` |
| Write Chinese contract | Docs | `doc/cn/contract/syscall_payload_v1.md` |
| Update contract index | Docs | `doc/{en,cn}/contract/README.md` |
| Define final byte layout for all WIT types | Design | Section 3 of this plan finalized |
| Define `uni-syscall.wit` function interface | Design | WIT functions replace payload-level `message_kind` table |

### Phase 2 — Extend `mudu_gen` to Generate the Project-Controlled Codec

| Task | File | Notes |
|------|------|-------|
| Parse WIT `variant` / `enum` / `record` / `list` / `option` / `result` / `box` | `mudu_gen/src/src_gen/wit_parser.rs` | Ensure all WIT constructs used by `mudu_binding/wit/` are supported |
| Generate Rust custom `serde` impls from WIT | `mudu_gen/src/lang_impl/rust/` or new module | Two-element `[tag, payload]` arrays, bare enum discriminants, transparent `box` |
| Generate C# custom MessagePack formatters from WIT | `mudu_gen/src/lang_impl/csharp/` | Mirror Rust encoding rules |
| Generate AssemblyScript encode/decode from WIT | `mudu_gen/src/lang_impl/assemblyscript/` | New backend |
| Add `mgen` integration test that round-trips WIT → generated Rust → MessagePack bytes | `mudu_gen/tests/` | Golden fixtures for each WIT type |
| Add `mgen` test for `uni-syscall.wit` function signatures | `mudu_gen/tests/` | Ensure generated bindings match WIT functions |

### Phase 3 — Rust Reference Codec (Generated from WIT)

| Task | File | Notes |
|------|------|-------|
| Generate Rust universal types from WIT | `mudu_binding/src/universal/` | Replace hand-written enums/records with `mgen` output where possible |
| Keep minimal hand-written conversion glue | `mudu_binding/src/universal/*_impl.rs` | Conversions to/from `mudu_type::DataType` / `DataValue` |
| Implement header encode/decode | `mudu_binding/src/codec/syscall_payload/mod.rs` | 16-byte header, magic/version validation |
| Wire WIT functions to internal header+body path | `mudu_binding/src/codec/syscall_payload/router.rs` | Map `query`/`command`/... to `message_kind` |
| Migrate SQL host handlers | `mudu_binding/src/system/query_invoke.rs`, `command_invoke.rs` | Use new codec |
| Migrate SQL in/outcoming | `mudu_binding/src/codec/handle_sys_incoming.rs`, `handle_sys_outcoming.rs` | Use new codec |
| Align KV encoding | `mudu_binding/src/codec/handle_sys_session.rs` | Unify conventions or keep as stable sub-format |
| Unit tests + fixtures | `mudu_binding/src/codec/syscall_payload/tests.rs` | Round-trip every WIT type and syscall |
| **Benchmark vs. `rmp_serde`** | `mudu_binding/benches/syscall_payload_bench.rs` | Acceptance criteria from Section 4 |

### Phase 4 — Rust Guest Codec

| Task | File | Notes |
|------|------|-------|
| Choose dependency model | — | Reuse `mudu_binding` or create `mudu_syscall_codec` |
| Generate/update Rust guest serialization | `mudu_api/rust/src/mudu_sys/mod.rs` | Remove direct `rmp_serde` calls; use generated codec |
| Guest round-trip tests | `mudu_api/rust/tests/` | Compile to wasm32 where applicable |

### Phase 5 — Code Generation for C# and AssemblyScript

| Task | File | Notes |
|------|------|-------|
| Extend `mudu_gen` schema input | `mudu_gen/src/src_gen/wit_parser.rs` | Already covered in Phase 2 |
| Generate C# codec from WIT | `mudu_gen/src/lang_impl/csharp/` | Replace `[MessagePackObject]` classes |
| Generate AS codec from WIT | `mudu_gen/src/lang_impl/assemblyscript/` | New backend |
| Update C# API surface | `mudu_api/csharp/uni/*.cs`, `MuduSysCallApi.cs` | Keep public methods, replace internals |
| Update AS runtime | `bindings/assemblyscript/assembly/*.ts` | Use raw byte syscalls |
| Simplify rs-shim | `bindings/rs-shim/src/*.rs` | Remove MessagePack path |

### Phase 5 — Versioning and MPK Integration

| Task | File | Notes |
|------|------|-------|
| Add migration skeleton | `mudu_binding/src/codec/syscall_payload/migrate/mod.rs` | Identity handler for v1 |
| Register in router | `mudu_kernel/src/compat.rs` | `FormatKind::SyscallPayload` |
| Add ABI version to MPK manifest | `mudu_runtime/src/service/app_package.rs` or manifest schema | Runtime checks version on load |
| Fail-fast version check | MPK loader | Clear error when guest ABI ≠ runtime ABI |

### Phase 6 — Cleanup and Validation

| Task | Command / File | Notes |
|------|----------------|-------|
| Remove `rmp_serde` from syscall path | `mudu_api/rust`, `mudu_api/csharp`, `bindings/rs-shim` | — |
| Format workspace | `cargo fmt --workspace` | — |
| Lint workspace | `cargo clippy --workspace --all-targets -- -D warnings` | — |
| Compile tests | `cargo test --no-run --workspace` | — |
| Run tests | `cargo test --workspace` | Focus on `mudu_binding`, `mudu_api/rust`, `mudu_runtime`, `testing` |
| Cross-language smoke test | `testing/tests/` | Rust, C#, AS guests against same runtime |
| Final benchmark report | `doc/en/contract/syscall_payload_v1.md` | Measured vs. `rmp_serde` |

---

## 7. Key Design Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Keep WIT as byte interface | Yes | Avoids wit-bindgen coupling; portable component boundary. |
| Express schema as canonical WIT | Yes | Single source of truth for all languages; `mgen` generates the codec. |
| Generate codec vs. hand-write | Generate from WIT via `mgen` | Guarantees parity across Rust/C#/AS; project controls MessagePack layout through templates. |
| Hand-write Rust encoder or generate it | Generate v1 from WIT, keep minimal hand-written glue | Rust is the reference; codegen ensures C#/AS parity and reduces manual drift. |
| Fixed-width vs. varint | Mixed: fixed-width for hot scalars, length-prefixed for blobs | Balance speed and compactness. |
| Backward compatibility for old MPK | No | Packages are rebuilt per ABI version; versioning is for forward control. |
| Runtime legacy decoders | No | Only current version is decoded; unsupported versions fail fast. |
| Performance gate | Yes | Benchmark acceptance criteria must pass before removing `rmp_serde`. |

---

## 8. Risks and Mitigations

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| Custom encoder slower than `rmp_serde` | Medium | High | Pre-size buffers, avoid per-field allocations, benchmark early, optimize hot paths before merge. |
| C#/AS generated code drifts from Rust | Medium | High | Single schema source + generated serializers. |
| Large blast radius across bindings | High | High | Land behind feature flag until tests pass; remove old path in final PR. |
| Subtle format bugs in nested `UniDataValue` | Medium | High | Exhaustive round-trip tests + golden fixtures. |
| `mudu_gen` not ready for WIT-driven serialization | Medium | High | Extend `wit_parser` and add generated-code round-trip tests as an early milestone before binding migration. |

---

## 9. Deliverables

1. Stabilized WIT schema under `mudu_binding/wit/`.
2. `uni-syscall.wit` function interface.
3. `SyscallPayload` v1 contract documents (English + Chinese).
4. `mudu_gen` extended to parse WIT and generate project-controlled MessagePack codecs for Rust/C#/AS.
5. Rust reference encoder/decoder in `mudu_binding` (generated from WIT, plus minimal hand-written glue).
6. Updated Rust guest API (`mudu_api/rust`) using the generated codec.
7. Updated C# guest API (`mudu_api/csharp`) using generated codec.
8. Updated AssemblyScript guest path using generated codec.
9. Simplified `rs-shim` without MessagePack.
10. MPK manifest ABI version check.
11. Unit tests, golden fixtures, cross-language round trips, and performance benchmark.
12. Benchmark report documenting performance vs. `rmp_serde`.
13. Developer documentation update.

---

## 10. Todo Checklist

### Phase 1 — WIT Schema, Format Contract and Registry
- [ ] Stabilize `mudu_binding/wit/*.wit` as the canonical schema.
- [ ] Add `uni-syscall.wit` with all syscall functions.
- [ ] Add `FormatKind::SyscallPayload` to `mudu/src/compat/mod.rs`.
- [ ] Add `SYSCALL_PAYLOAD_CURRENT_VERSION` and `CompatibilityMatrix` entries.
- [ ] Assign magic value `0x4D53_5350`.
- [ ] Write `doc/en/contract/syscall_payload_v1.md`.
- [ ] Write `doc/cn/contract/syscall_payload_v1.md`.
- [ ] Update `doc/en/contract/README.md` and `doc/cn/contract/README.md`.
- [ ] Finalize body encoding rules for all WIT types.

### Phase 2 — Extend `mudu_gen`
- [ ] Ensure `mudu_gen` parses all WIT constructs used by `mudu_binding/wit/`.
- [ ] Generate Rust custom `serde` implementations from WIT.
- [ ] Generate C# custom MessagePack formatters from WIT.
- [ ] Generate AssemblyScript encode/decode functions from WIT.
- [ ] Add `mgen` integration tests with MessagePack golden fixtures.
- [ ] Add `mgen` test for `uni-syscall.wit` function signatures.

### Phase 3 — Rust Reference Codec (Generated from WIT)
- [ ] Generate Rust universal types from WIT into `mudu_binding/src/universal/`.
- [ ] Keep minimal hand-written conversion glue in `*_impl.rs`.
- [ ] Create `mudu_binding/src/codec/syscall_payload/mod.rs` for header encode/decode.
- [ ] Wire WIT functions to internal header+body routing.
- [ ] Update `mudu_binding/src/system/query_invoke.rs`.
- [ ] Update `mudu_binding/src/system/command_invoke.rs`.
- [ ] Update `mudu_binding/src/codec/handle_sys_incoming.rs`.
- [ ] Update `mudu_binding/src/codec/handle_sys_outcoming.rs`.
- [ ] Align or unify `mudu_binding/src/codec/handle_sys_session.rs` conventions.
- [ ] Add unit tests for every WIT type and syscall.
- [ ] Add golden fixtures for v1.
- [ ] Add `mudu_binding/benches/syscall_payload_bench.rs`.
- [ ] Measure and document performance vs. `rmp_serde`.

### Phase 4 — Rust Guest
- [ ] Decide `mudu_binding` vs. new `mudu_syscall_codec` crate.
- [ ] Generate/update Rust guest serialization in `mudu_api/rust/src/mudu_sys/mod.rs`.
- [ ] Add guest-side round-trip tests.
- [ ] Remove `rmp_serde` dependency from `mudu_api/rust` syscall path.

### Phase 5 — C# and AssemblyScript
- [ ] Generate C# codec from WIT via `mudu_gen`.
- [ ] Generate AS codec from WIT via `mudu_gen`.
- [ ] Generate or rewrite `mudu_api/csharp/uni/*.cs`.
- [ ] Update `mudu_api/csharp/mudu_sys/MuduSysCallApi.cs`.
- [ ] Update `bindings/assemblyscript/assembly/*.ts`.
- [ ] Simplify `bindings/rs-shim/src/facade.rs` and related files.
- [ ] Remove MessagePack from `bindings/rs-shim`.

### Phase 5 — Versioning and MPK
- [ ] Create `mudu_binding/src/codec/syscall_payload/migrate/mod.rs`.
- [ ] Register handler in `mudu_kernel/src/compat.rs`.
- [ ] Add syscall ABI version field to MPK manifest.
- [ ] Implement fail-fast version check in MPK loader.
- [ ] Add tests for version mismatch error paths.

### Phase 6 — Validation and Cleanup
- [ ] Remove `rmp_serde` from all syscall paths.
- [ ] Run `cargo fmt --workspace`.
- [ ] Run `cargo clippy --workspace --all-targets -- -D warnings`.
- [ ] Run `cargo test --no-run --workspace`.
- [ ] Run `cargo test --workspace`.
- [ ] Run cross-language smoke tests.
- [ ] Ensure benchmark acceptance criteria are met.
- [ ] Update developer documentation.
- [ ] Final review and merge.

---

## 11. Recommendation

Proceed with the schema-first self-owned binary ABI. It gives MuduDB full control over the guest→host syscall wire format, reuses the existing compatibility infrastructure, avoids a per-language manual implementation, and keeps the WIT boundary minimal. The format is designed to be faster than `rmp_serde` on representative workloads by eliminating serde trait overhead, using fixed-width hot fields, and removing MessagePack framing. Performance is gated by explicit acceptance criteria and a criterion benchmark before the old path is removed. Legacy MPK packages are not supported across ABI version bumps; packages are rebuilt for the matching runtime version.
