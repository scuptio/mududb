# mudu_runtime

`mudu_runtime` provides the serving runtime infrastructure for MuduDB
applications: pgwire/HTTP session handling and process management. The
WebAssembly procedure host lives in the separate `runtime_wasm` crate, and
the libsql connector in `runtime_libsql`.

It depends on the narrow kernel crates (`server`, `worker`, `xengine`,
`wal`, `repl`, `catalog`, `meta`, `conn`, `backup`) directly, not on the
`mudu_kernel` facade.

## Responsibility

- Host the pgwire and HTTP serving backends, accept loops, and session
  handling (`backend`).
- Serve the HTTP management API and manage installed applications
  (`backend::http_api`, `backend::app_mgr`, `backend::mudu_app_mgr`).
- Load and validate server configuration (`backend::mudud_cfg`,
  `backend::cfg_meta`).
- Provide the Linux io_uring server backend (`backend::server_ur`).

## What does NOT belong here

- WebAssembly procedure hosting (component loading and instantiation,
  guest/host interface bindings, app package management, schema
  resolution) — that belongs to `runtime_wasm`.
- libsql-backed database connector drivers — those belong to
  `runtime_libsql`.
- Low-level page/storage format details — those belong to the `storage`
  crate.
- Shared wire-format types — those belong to `mudu_contract`.
- SQL parsing and AST construction — that belongs to `sql_parser`.
- Code generation from DDL/WIT — that belongs to `mudu_gen`.

## Main public entry points

- `mudu_runtime::backend` — backend server implementations, session
  handling, and HTTP API hosting.
