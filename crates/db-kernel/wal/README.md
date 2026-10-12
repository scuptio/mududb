# wal

Write-ahead log format, serialization, and per-worker backend.

## Role

The WAL layer records logical and physical operations so that worker state
can be recovered after a crash. In the kernel dependency chain defined in
[ARCHITECTURE.md](../../../ARCHITECTURE.md) it sits above
`crates/db-kernel/catalog` and below `crates/db-kernel/xengine`, and is
consumed across the kernel — by the engine path (`xengine`, `storage`,
`command`, `sql`) and by the server-side crates (`worker`, `xcontract`,
`repl`, `backup`) plus `mudu_runtime` and the `testing` helpers.

## Key modules

- `format` — versioned on-disk log frame formats; `log_frame` re-exports the
  current one (`format::latest`), and `migrate` hosts the migration handlers
  between frame formats.
- `lsn` — log sequence number.
- `xl_entry` / `xl_data_op` / `xl_batch` — transaction log entries, their
  logical row-level write payloads (insert, update, delete), and batches of
  entries.
- `pl_entry` / `pl_batch` — physical log entries describing file/page
  mutations, and batches of entries.
- `xl_batch_worker_log` / `pl_batch_worker_log` — worker-log specializations
  of the transaction/physical batches.
- `typed_worker_log` — a worker log typed on its entry payload.
- `worker_log` — the worker log backend traits and frame decoding helpers.
- `commit_ts` — commit timestamps stamped on committed transactions.
- `worker_wal_backend` (private) — the per-worker backend behind the
  `worker_log` traits: chunked log layout (`layout.rs`), append state
  (`state.rs`), batching watermarks (`batching.rs`), durability policy
  (`sync_policy.rs`), and the flush pipeline under `flush/`, split by
  pipeline stage — `group_commit.rs` (append and group-commit wait),
  `driver.rs` (flush-round scheduling and completion), and `io.rs`
  (drain/merge of pending batches, write and fsync execution).

## Features

- `test` — test helpers; dev-dependencies only, production dependencies must
  not enable it (enforced by `script/ci/check_test_feature.py`).
- `debug_trace` — enables the `mudu_utils` debug-trace instrumentation (HTTP
  task-trace dump endpoint).
