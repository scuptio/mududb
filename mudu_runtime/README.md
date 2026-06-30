# mudu_runtime

`mudu_runtime` provides the hosted runtime for MuduDB applications and stored
procedures. It builds on top of `mudu_kernel` and `wasmtime` to load `.mpk`
packages, set up WASI contexts, and expose the database API to guest code over
the guest/host interface.

## Responsibility

- Load, validate and install Mudu packages (`service`).
- Manage Wasmtime instances, components, stores and WASI contexts (`service`,
  `procedure`).
- Host HTTP API servers and session management (`backend`).
- Provide database connector traits and libsql-backed drivers
  (`db_connector`, `db_libsql`, `db_libsql_async`).
- Resolve schemas and bind guest procedures to kernel operations (`resolver`).
- Define the guest/host interface contracts (`interface`).

## What does NOT belong here

- Low-level page/storage format details — those belong to `mudu_kernel`.
- Shared wire-format types — those belong to `mudu_contract`.
- SQL parsing and AST construction — that belongs to `sql_parser`.
- Code generation from DDL/WIT — that belongs to `mudu_gen`.

## Main public entry points

- `mudu_runtime::service` — package loading, runtime services and Wasmtime
  context setup.
- `mudu_runtime::backend` — HTTP serving, session handling and backend process
  management.
- `mudu_runtime::db_connector` — traits for connecting to a MuduDB backend.
- `mudu_runtime::interface` — guest/host interface types.
- `mudu_runtime::resolver` — schema and procedure resolution helpers.
