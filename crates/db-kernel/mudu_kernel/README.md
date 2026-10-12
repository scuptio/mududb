# mudu_kernel

`mudu_kernel` is the facade crate of the MuduDB database kernel. Since the
kernel split, the implementation lives in the narrow kernel crates
(`catalog`, `index`, `wal`, `xengine`, `storage`, `meta`, `sql`, `conn`,
`server`, ...); `mudu_kernel` itself is a pure compatibility facade that
re-exports them under the historical `mudu_kernel::*` module paths, plus
the `compat` helpers (global router installation). It sits directly above
the storage and network layers and exposes the SQL planner/binder,
transaction manager, write-ahead log, B-tree indexes, catalog managers, and
the public cross-engine API.

## Responsibility

- Re-export the narrow kernel crates under the historical module paths
  (`contract`, `index`, `meta`, `mudu_conn`, `sql`, `wal`, `server`,
  `storage`, `x_engine`) and provide compatibility helpers (`compat`).
- Parse, bind, plan and execute SQL statements (`sql`).
- Manage on-disk relation and time-series storage (`storage`).
- Provide transaction concurrency primitives and WAL recovery (`wal`).
- Maintain in-memory catalogs for schemas, partitions and placements (`meta`).
- Run the network server, protocol handlers and worker runtime (`server`).
- Expose the primary `MuduEngine` API used by adapters and clients (`x_engine`).
- Host fuzzing harnesses for stable on-disk formats (`fuzz`).

## What does NOT belong here

- Language bindings, CLI tools or HTTP frontends — those live in
  `mudu_binding`, `mudu_cli` and `mudu_runtime`.
- Guest/host Wasmtime glue — that belongs to `runtime_wasm`.
- Pure contract/value types that are shared between client and server — those
  belong to `mudu_contract`.
- SQL grammar and AST definitions — those belong to `sql_parser`.

## Main public entry points

- `mudu_kernel::x_engine` — the cross-engine `MuduEngine` surface.
- `mudu_kernel::server` — server runtime and protocol handlers.
- `mudu_kernel::storage` — page, relation and time-series storage.
- `mudu_kernel::sql` — SQL binder, planner and statement execution.
- `mudu_kernel::mudu_conn` — async connection and prepared-statement wrappers.
