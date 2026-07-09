# mudu

Pure foundation crate for MuduDB. `mudu` provides common types, error
codes, macros, SQL-like data type definitions, serialization helpers and
format-compatibility checks used by the rest of the workspace. It performs
no I/O and has no dependency on `mudu_sys`.

## Responsibility

- Common workspace types and helpers (`common`).
- Error types, error codes and convenience macros (`error`).
- SQL-like data type definitions such as date/time/timestamp/numeric (`data_type`).
- Format-compatibility registry and version checks (`compat`).
- Pure utility helpers for JSON, MessagePack, TOML, buffers and case conversion (`utils`).

## What does NOT belong here

- Storage engine, persistent I/O and system calls: live in `mududb`, `mudu_sys`,
  `mudu_sys_impl`.
- WASM runtime / host bindings: live in `mudu_wasm`, `mudu_sys_wasm`.
- SQL parsing: live in `sql_parser`.
- Language bindings and adapters: live in `mudu_binding`, `mudu_adapter`.
- Kernel, runtime and transaction orchestration: live in `mudu_kernel`,
  `mudu_runtime`.
- CLI, package management and codegen: live in `mudu_cli`, `mpm_build`,
  `mudu_gen`.

## Main public entry points

- `mudu::common` — shared types and helpers (IDs, OIDs, slices, serde utilities,
  `UpdateDelta`, etc.).
- `mudu::compat` — `FormatKind`, `CompatibilityMatrix`, `CompatError`, and
  `check_magic` / `check_version` helpers.
- `mudu::data_type` — `DateValue`, `TimeValue`, `TimestampValue`,
  `TimestampTzValue`, `Numeric`, and temporal helpers.
- `mudu::error` — `MuduError`, `ErrorCode`, `Severity`, `ResultExt`, and the
  `bail!` / `ensure!` / `mudu_error!` macros.
- `mudu::utils` — pure helpers for JSON, MessagePack, TOML, sized buffers and
  case conversion.
