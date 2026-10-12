# sql

SQL parsing, binding, planning, and statement execution.

## Role

The SQL layer turns raw SQL text into bound statement trees, optimizes them
into execution plans, and drives the command and executor modules. In the
kernel dependency chain defined in
[ARCHITECTURE.md](../../../ARCHITECTURE.md) it sits above
`crates/db-kernel/command` and below `crates/db-kernel/conn`, and is consumed
by the connection layer plus `worker`, `xcontract`, `server`, and `backup`.
SQL text parsing itself is delegated to the `sql_parser` crate in
`crates/common`.

## Key modules

- `binder` — binds parsed SQL statements against the catalog into bound
  statements.
- `bound_stmt` — bound statement trees produced by the binder.
- `bound_template` — parameter templates produced by template-mode binding.
- `select_projection` — shared binding logic for select-list items (columns
  and aggregates).
- `describer` — computes the result tuple descriptor of a statement without
  executing it.
- `planner` — turns bound statements into query and command executors.
- `plan_ctx` — context bundle shared by the executors the planner produces.
- `executor` — query executors that access and filter table data:
  `index_access_key` (key-equality point reads), `index_access_range`
  (key-range scans), `aggregate` (whole-set aggregates), `filter` (residual
  non-key filters), and `value_compare` (decoded datum comparison).
- `copy_layout` — column-to-CSV layout resolution for COPY FROM and COPY TO.
- `value_codec` — encoding of SQL literals and parameters into binary column
  datums.

## Features

- `test` — test helpers; dev-dependencies only, production dependencies must
  not enable it (enforced by `script/ci/check_test_feature.py`).
- `debug_trace` — enables the `mudu_utils` debug-trace instrumentation (HTTP
  task-trace dump endpoint).
