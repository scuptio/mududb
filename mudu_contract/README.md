# mudu_contract

`mudu_contract` holds the shared types and protocols that cross the boundary
between MuduDB clients and servers. It is deliberately low-level: it defines
how tuples, sessions, SQL statements, procedure descriptors and wire frames are
represented, but it does not implement execution.

## Responsibility

- Define database session, connection and SQL abstractions (`database`).
- Describe procedure signatures, parameters and result shapes (`procedure`).
- Provide the client/server wire protocol frames and request/response types
  (`protocol`).
- Specify tuple layout, binary encoding, field descriptors and conversion
  utilities (`tuple`).
- Export the `sql_stmt!` and `sql_params!` pass-through macros.

## What does NOT belong here

- SQL parsing or query planning — that belongs to `sql_parser` and
  `mudu_kernel`.
- Storage engine implementation — that belongs to `mudu_kernel`.
- Wasmtime runtime or package loading — that belongs to `mudu_runtime`.
- System-level host implementations — those belong to `mudu_sys_impl`.

## Main public entry points

- `mudu_contract::database` — session/connection abstractions and SQL types.
- `mudu_contract::procedure` — procedure descriptors and parameter metadata.
- `mudu_contract::protocol` — wire protocol frames.
- `mudu_contract::tuple` — tuple encoding, decoding and layout utilities.
- `mudu_contract::sql_stmt!` / `mudu_contract::sql_params!` — statement and
  parameter expression macros.
