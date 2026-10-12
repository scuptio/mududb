# xengine

Public cross-engine API and the main `MuduEngine` implementation.

## Role

`xengine` exposes the kernel surface used by adapters and clients: catalog
operations, tuple cursors, transaction management, and KV-style data access.
In the kernel dependency chain defined in
[ARCHITECTURE.md](../../../ARCHITECTURE.md) it sits above
`crates/db-kernel/wal` and below `crates/db-kernel/storage`, and is consumed
by the engine path above it (`storage`, `meta`, `command`, `sql`, `conn`) as
well as the server-side crates (`worker`, `xcontract`, `server`, `backup`)
and the runtime crates (`mudu_runtime`, `runtime_wasm`).

## Key modules

- `api` — storage-facing execution contract and its parameter and result
  types.
- `operator` — column filter operators applied to binary-encoded values.
- `partition_router` — routing of keys and key ranges to table partitions.
- `tx_mgr` — transaction-local staging of a transaction's writes and
  snapshot.
- `x_param` — parameter bundles consumed by the engine commands and
  executors.

## Features

- `debug_trace` — enables the `mudu_utils` debug-trace instrumentation (HTTP
  task-trace dump endpoint).
