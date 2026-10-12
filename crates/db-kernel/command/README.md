# command

Command interpreters for DDL and DML.

## Role

`command` turns bound DDL/DML statements into calls against the layers below
it — `crates/db-kernel/meta`, `crates/db-kernel/xengine`,
`crates/db-kernel/wal`, and `crates/db-kernel/catalog`. In the kernel
dependency chain defined in [ARCHITECTURE.md](../../../ARCHITECTURE.md) it
sits below `crates/db-kernel/sql`, and is consumed by the SQL layer and by
`crates/db-kernel/backup`.

## Key modules

- `create_table` / `drop_table` — create-table and drop-table command
  executors.
- `create_fs_type` / `drop_fs_type` — create/drop-filesystem-type command
  executors.
- `create_partition_rule` — create-partition-rule command executor.
- `create_partition_placement` — create-partition-placement command executor.
- `insert_key_value` / `update_key_value` / `delete_key_value` — insert,
  update-by-key, and delete-by-key command executors.
- `load_from_file` / `save_to_file` — load-from-file (COPY FROM) and
  save-to-file (COPY TO) command executors.
- `fs_hook` — DML hooks that create and bind filesystem objects for FS-bound
  columns.

## Features

- `test` — test helpers; dev-dependencies only, production dependencies must
  not enable it (enforced by `script/ci/check_test_feature.py`).
- `debug_trace` — enables the `mudu_utils` debug-trace instrumentation (HTTP
  task-trace dump endpoint).
