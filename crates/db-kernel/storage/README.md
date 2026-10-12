# storage

On-disk storage layer: pages, relations, and time-series files.

## Role

`storage` sits in the middle of the kernel dependency chain defined in
[ARCHITECTURE.md](../../../ARCHITECTURE.md): it builds on
`crates/db-kernel/catalog`, `crates/db-kernel/index`,
`crates/db-kernel/wal`, and `crates/db-kernel/xengine`, and is consumed by
`crates/db-kernel/meta` (and transitively the engine path above it) plus the
server-side crates (`worker`, `xcontract`, `repl`, `server`) and the
`testing` helpers.

## Key modules

- `page` — slotted-page primitives: the header and tailer codec, record
  slots, and the read-only and mutable page-block views over a page buffer.
- `relation` — MVCC row layer over two time-series files (keys and values)
  with an in-memory index. The per-relation implementation lives in
  `relation/relation/`, split by concern: `open.rs` (construction and
  recovery rebuild from the durable files), `read.rs` (MVCC
  visible-meta/value/range reads and write-conflict detection), and
  `write.rs` (row and delta writes, tuple-id allocation).
- `time_series` — time-series file storage: versioned records in a chain of
  slotted pages.
- `prepared_commit` — prepared commit payload and the commit-sequencer
  interface.
- `tuple_build` — tuple-building and delta-assignment codec helpers shared by
  the storage and cross-contract layers.
- `value_update` — tuple value-update helpers shared by the storage and
  cross-contract layers.

## Features

- `test` — test helpers; dev-dependencies only, production dependencies must
  not enable it (enforced by `script/ci/check_test_feature.py`).
- `debug_trace` — enables the `mudu_utils` debug-trace instrumentation (HTTP
  task-trace dump endpoint).
