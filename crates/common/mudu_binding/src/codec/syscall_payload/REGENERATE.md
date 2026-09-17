# Regenerating the MSSP syscall codec

The per-func syscall codec (`generated/uni_syscall.rs`) and the universal DTOs
(`src/universal/uni_*.rs`) are produced by `mudu_gen` (`mgen`) from the WIT
contracts in `crates/common/mudu_binding/wit`. Do not edit the generated files
manually.

## Wire format (MSSP v1, in-place revision)

- A request body is a MessagePack **map keyed by the 1-based parameter
  numbers**; a record body is a map keyed by the 1-based field numbers (WIT
  declaration order). Decode is lenient: unknown keys are skipped, missing
  fields default (proto3 semantics), integers of any width are accepted.
- Result bodies stay `[0u8, value]` / `[1u8, UniError]`; variants stay
  `[tag, payload]` and reject unknown tags.
- All body values are encoded/decoded by the hand-written `mp_wire` runtime
  (`src/universal/mp_wire.rs`), which is byte-aligned with `rmp_serde` 1.3.1
  (minimal-width integers, str8/bin8 thresholds). Generated DTOs are plain
  data plus `to_value` / `from_value` conversions against that runtime.
- `UniError.err_details` and `UniResultSet.cursor` keep encoding as integer
  arrays (record-context `list<u8>`), not bin.

## Command

From `crates/tools`:

```sh
cargo run -p mudu_gen -- message \
    -i ../common/mudu_binding/wit \
    -o <scratch dir> \
    -l rust --with-func-codec \
    -t <scratch dir>/syscall_schema.desc.json
```

## Where the output lands

| Generated file (`<scratch dir>/`) | Copied to (`crates/common/mudu_binding/src/`) |
| --- | --- |
| `uni_syscall.rs` | `codec/syscall_payload/generated/uni_syscall.rs` |
| `uni_<name>.rs` (most types) | `universal/uni_<name>.rs` (same name) |
| `uni_command_result.rs` | `universal/uni_command_result.rs` |
| `uni_query_result.rs` | `universal/uni_query_result.rs` (also contains `UniQueryReturn`) |
| `uni_message.rs` | **not imported** — no host-side consumer today |
| `syscall_schema.desc.json` | `codec/syscall_payload/syscall_schema.desc.json` (embedded via `include_str!` in `syscall_payload::SYSCALL_SCHEMA_DESC_JSON`) |

`universal/mp_wire.rs` is the hand-written wire runtime the generated code
references (`crate::universal::mp_wire`); it is NOT generated. The mirrored
copy for the standalone SDK is synced automatically by `mudu_api_sync`.

Compatibility shims kept for the pre-generation module paths (do not delete):

- `universal/uni_command_return.rs` re-exports `UniCommandResult` /
  `UniCommandReturn` from `uni_command_result`.
- `universal/uni_query_return.rs` re-exports `UniQueryReturn` from
  `uni_query_result`.

The generated `uni_syscall.rs` refers to the universal types through
`crate::universal::...` paths, so it works unchanged as long as the table
above is followed.

`universal/uni_serde.rs` holds hand-written **legacy serde** impls for the
types with JSON-facing consumers (the `syscall_schema` descriptor,
`UniTypeDesc`, the JSON client, the management/topology JSON, the procedure
HTTP JSON). They are never used on the syscall wire.

## WIT pinning notes

- `uni-syscall.wit` declaration order **is** the wire `message_kind`
  numbering (1–23). Never reorder it.
- Record field order is the wire field numbering (`rf_number`, 1-based).
  Appending fields is wire-compatible; reordering or renumbering is not.
- `uni-scalar.wit` discriminants are the deployed wire values: `u128` follows
  `u64`, `i128` follows `i64`, and the last case is `timestamp-tz`
  (PascalCase `TimestampTz`). `uni-scalar-value.wit` uses the same case name.
- `uni-data-type.wit` ends with `%box(box<uni-data-type>)` (wire tag 8). The
  `Box` variant doubles as `mudu_gen`'s internal IR for WIT `box<T>` types,
  so it must stay even though no syscall payload carries it.

## After regenerating

Run, in order:

```sh
cd crates/common     && cargo test -p mudu_binding
cd crates/db-kernel  && cargo test -p testing --test compat_golden
cd crates/db-kernel  && cargo test -p mudu_runtime
cd crates/common     && cargo test -p sys_interface
cd crates/db-kernel  && cargo test -p testing --test wallet_mpk \
                        --test wallet_as_mpk --test wallet_cs_mpk
```

The `compat_golden` fixtures
(`crates/db-kernel/testing/fixtures/golden/v1/*.bin`) are the byte-exact pin.
When the wire format is intentionally revised, regenerate them once with

```sh
cargo test -p testing --test compat_golden -- generate_golden_v1_fixtures --ignored
```

and review the diff; for accidental byte changes, stop and reconcile the WIT
or the templates instead of regenerating.
