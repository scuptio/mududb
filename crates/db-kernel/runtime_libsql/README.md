# runtime_libsql

libsql-based database connector: connection, transaction, and result-set
drivers used by the runtime to reach external libsql/SQLite databases.

## Role

Recently split out of `mudu_runtime`. Consumed by `runtime_wasm` and
`mudu_runtime` (the pgwire/HTTP serving backend); builds on `conn` for the
MuduDB backend and on the external `libsql` crate for LibSQL/SQLite backends.

## Key modules

- `db_connector` — database connection helpers for LibSQL and MuduDB
  backends; `DBConnector` is the factory selecting the backend (`LibSQL`,
  `LibSQLAsync`, or `MuduDB`) from a connection string.
- `db_libsql` — synchronous libsql connection, transaction, and result-set
  drivers (private implementation).
- `db_libsql_async` — asynchronous libsql connection driver (private
  implementation).

## Features

- `ds` — pass-through for `mudu_sys/ds` (deterministic-simulation backend; a
  no-op in this repository). Tests that exercise real-OS semantics the
  simulator cannot provide (e.g. libsql's real file I/O) are gated behind
  `#[cfg(not(feature = "ds"))]`.
