# catalog

Kernel catalog: kernel-facing contracts, value types, shared id types, and
fuzzing helpers shared across the kernel subsystems.

## Role

`catalog` is the bottom of the kernel dependency chain
(`conn -> sql -> command -> meta -> storage -> xengine -> wal -> index ->
catalog`) defined in [ARCHITECTURE.md](../../../ARCHITECTURE.md). It depends
only on the common crates (`mudu`, `mudu_contract`, `mudu_sys`, `mudu_type`,
`mudu_utils`) and is consumed by every other crate in `crates/db-kernel`.

## Key modules

- `common` — shared helpers and newtype id macros.
- `contract` — kernel-facing contracts and value types exchanged between
  subsystems (catalog descriptors, command/query parameter and result types).
- `frame` — frame-decoding helpers shared by the server and replication
  layers.
- `repl_cfg` — kernel-side replication configuration types.
- `repl_ctrl` — replication-control interfaces exposed to the
  worker/transaction layers.
- `stage_stats` — lightweight per-stage latency accounting for performance
  analysis.

## Features

- `test` — test helpers; dev-dependencies only, production dependencies must
  not enable it (enforced by `script/ci/check_test_feature.py`).
- `debug_trace` — enables the `mudu_utils` debug-trace instrumentation (HTTP
  task-trace dump endpoint).
