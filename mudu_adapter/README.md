# mudu_adapter

Database adapter that routes Mudu storage operations to the configured backend. It supports SQLite, PostgreSQL, MySQL, and the remote Mudud protocol, exposing a uniform synchronous and asynchronous API that returns `mudu::common::result::RS`.

## Responsibility

- Routing session, key-value, query, and command operations to the active backend driver.
- Parsing `MUDU_CONNECTION` into a typed `ConnectionConfig` and selecting the matching `Driver`.
- Implementing per-backend drivers for SQLite (`sqlite`), PostgreSQL (`postgres`), MySQL (`mysql`), and the remote Mudud protocol (`mududb`).
- Materializing query results into an in-memory `LocalResultSet`.
- Providing SQL parameter conversion and placeholder replacement helpers.

## What does NOT belong here

- Core result / error types live in `mudu`.
- Storage engine internals and transaction management live in `mudu` (the core crate).
- Remote protocol client implementation (`mudu_cli`) and contract types (`mudu_contract`) live in their respective crates.
- Type system and datum conversions are owned by `mudu_type`.
- OS-level task, sync primitives, and environment helpers are provided by `mudu_sys`.

## Main public entry points

- `mudu_adapter::syscall` — top-level public API (`mudu_open`, `mudu_close`, `mudu_get`, `mudu_put`/`mudu_set`, `mudu_range`, `mudu_query`, `mudu_command`, `mudu_batch`, plus async variants).
- `mudu_adapter::backend` — backend dispatcher that implements the same operations by routing to the configured driver.
- `mudu_adapter::config` — connection configuration (`Driver`, `ConnectionConfig`), `MUDU_CONNECTION` parsing, and SQLite path override helpers.
- `mudu_adapter::result_set` — `LocalResultSet` for materializing rows.
- `mudu_adapter::sql` — SQL parameter conversion and placeholder replacement.
- `mudu_adapter::kv` — SQLite-specific key-value helpers.
- `mudu_adapter::state` — adapter-local session ID generation.
- `mudu_adapter::codec` — encoding/decoding helpers used by backend implementations.
