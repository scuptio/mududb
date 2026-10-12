# meta

In-memory catalog managers for schemas, partitions, rules, and placements.

## Role

`meta` keeps the system-table catalog in memory. In the kernel dependency
chain defined in [ARCHITECTURE.md](../../../ARCHITECTURE.md) it sits above
`crates/db-kernel/storage` and below `crates/db-kernel/command`, and is
consumed by the command/execution layers above it plus the server-side
crates (`worker`, `xcontract`, `server`, `backup`) and `mudu_runtime`.

## Key modules

- `meta_mgr` — the in-memory catalog manager over the system tables.
- `meta_mgr_factory` — factory building the in-memory catalog manager.
- `schema_catalog` — catalog of table schemas.
- `partition_binding_catalog` — catalog of table partition bindings.
- `partition_rule_catalog` — catalog of partition rules.
- `partition_placement_catalog` — catalog of partition placements
  (partition-to-worker assignments).
- `fs_type_catalog` — catalog of registered filesystem types.
- `fs_object` — the `_fs_object` system table backing FS-bound columns.
- `_fuzz` — fuzz helpers generating schema-conformant rows (test/fuzz builds
  only).

## Features

- `test` — test helpers; dev-dependencies only, production dependencies must
  not enable it (enforced by `script/ci/check_test_feature.py`).
- `debug_trace` — enables the `mudu_utils` debug-trace instrumentation (HTTP
  task-trace dump endpoint).
