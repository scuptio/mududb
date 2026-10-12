# conn

Async connection, prepared-statement, and result-set wrappers — the
client-facing API over the kernel engine, exposing a `pgwire`-compatible
connection surface.

## Role

Kernel-layer crate bridging the SQL engine and the server layer: consumed by
`worker`, `server`, `runtime_libsql`, and `runtime_wasm`, and re-exported
through the `mudu_kernel` facade. Builds on `sql`, `catalog`, `xengine`, and
`sql_parser` (see [ARCHITECTURE.md](../../../ARCHITECTURE.md)).

## Key modules

- `mudu_conn_async` — async connection facade over the kernel engine
  (`MuduConnAsync`).
- `mudu_conn_core` — shared connection core: statement execution helpers.
- `mudu_prepared_stmt` — prepared-statement wrapper.
- `mudu_result_set_async` — async result-set wrapper over query cursors
  (`MuduResultSetAsync`).
- `plan_cache` — catalog-versioned cache of bound parameter templates (the
  "plan cache").
- `stmt_parse_cache` — process-wide cache for parsed SQL statements.
- `worker_local` — worker-local session state.

## Features

- `test` — test helpers and fixtures; may only be enabled by dev-dependencies,
  never by production dependencies (enforced by
  `script/ci/check_test_feature.py`).
- `debug_trace` — enables extra debug tracing via `mudu_utils/debug_trace`.
