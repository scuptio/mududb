# xcontract

Kernel contract prelude: the worker-side execution contract and every shared
type the kernel crates consume.

`WorkerXContract` is the per-worker implementation of the `XContract` storage
engine contract: transactional relation DML and worker-local KV, WAL-backed
commit, and cross-partition routing over the message bus. The crate re-exports
the contract traits and their parameter/result types from the lower kernel
layers so kernel crates depend on this single prelude.

## Role

Base of the server stack in `crates/db-kernel` (`server -> backup -> repl ->
worker -> xcontract`, see [ARCHITECTURE.md](../../../ARCHITECTURE.md)).
Consumed by `worker`, `repl`, `server`, and the `mudu_kernel` facade;
aggregates and re-exports the contract surface of `xengine`, `catalog`,
`meta`, `storage`, and `wal`.

## Key modules

- `trait_impl` — the `XContract` trait implementation for `WorkerXContract`.
- `lifecycle` — construction, initialization, and replication wiring of
  `WorkerXContract`.
- `kv` — worker-local KV operations and the transaction commit paths.
- `ops` — relation DML implementations (insert / read / update / delete).
- `rpc` / `partition_rpc` — the partition RPC handler and sender, the routed
  commit paths, and the wire messages exchanged between workers.
- `worker_storage` — per-worker storage: relation indexes, MVCC KV rows, and
  commit apply.
- `worker_tx_manager` — transaction state staging behind the `TxMgr` trait.
- `x_lock_mgr` — statement/commit key lock manager with FIFO-fair bounded
  waiting.
- `cursor` — in-memory vector-backed cursor used as the `RSCursor` for range
  reads.
- `params` — parameter bundles for constructing a `WorkerXContract`.

## Features

- `test` — test helpers and fixtures; may only be enabled by dev-dependencies,
  never by production dependencies (enforced by
  `script/ci/check_test_feature.py`).
- `debug_trace` — enables extra debug tracing via `mudu_utils/debug_trace`.
- `ds` — pass-through for `mudu_sys/ds` (deterministic-simulation backend; a
  no-op in this repository).
