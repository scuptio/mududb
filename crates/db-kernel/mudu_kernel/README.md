# mudu_kernel

`mudu_kernel` is the core database engine of MuduDB. It sits directly above the
storage and network layers and implements the SQL planner/binder, transaction
manager, write-ahead log, B-tree indexes, catalog managers, and the public
cross-engine API.

## Responsibility

- Parse, bind, plan and execute SQL statements (`sql`, `command`, `executor`).
- Manage on-disk relation and time-series storage (`storage`).
- Provide transaction concurrency primitives and WAL recovery (`wal`, `tx_mgr`).
- Maintain in-memory catalogs for schemas, partitions and placements (`meta`).
- Run the network server, protocol handlers and worker runtime (`server`).
- Expose the primary `MuduEngine` API used by adapters and clients (`x_engine`).
- Host fuzzing harnesses for stable on-disk formats (`fuzz`).

## What does NOT belong here

- Language bindings, CLI tools or HTTP frontends — those live in
  `mudu_binding`, `mudu_cli` and `mudu_runtime`.
- Guest/host Wasmtime glue — that belongs to `mudu_runtime`.
- Pure contract/value types that are shared between client and server — those
  belong to `mudu_contract`.
- SQL grammar and AST definitions — those belong to `sql_parser`.

## Main public entry points

- `mudu_kernel::x_engine` — the cross-engine `MuduEngine` surface.
- `mudu_kernel::server` — server runtime and protocol handlers.
- `mudu_kernel::storage` — page, relation and time-series storage.
- `mudu_kernel::sql` — SQL binder, planner and statement execution.
- `mudu_kernel::mudu_conn` — async connection and prepared-statement wrappers.
