# mudu_utils

Cross-cutting utility crate that provides thin, typed wrappers and facades over low-level primitives from `mudu_sys` and `mudu`. It covers serialization helpers, hashing, ID generation, logging setup, async/blocking task runtimes, task/thread tracing, and synchronization/notification primitives.

## Responsibility

- File-based JSON and TOML serialization helpers.
- MD5 digest helper.
- OID/XID generation based on UUIDv4.
- Logging initialization (`tracing` setup with optional console subscriber).
- Async and blocking task/runtime facades.
- Task context, task IDs, and task/thread tracing utilities.
- Async cancellation and notification primitives.
- Re-export of synchronization primitives from `mudu_sys`.
- Source-file path resolution helper (`this_file!`).
- Optional HTTP debug server for dumping task traces (behind `debug_trace` feature).

## What does NOT belong here

- Core type system / schema definitions live in `mudu_type`.
- SQL parsing lives in `sql_parser`.
- Database kernel and storage engine concerns live in `mudu_kernel` and `mududb`.
- Adapter and binding code lives in `mudu_adapter` and `mudu_binding`.
- Code generation lives in `mudu_gen`.
- Transpiler logic lives in `mudu_transpiler`.
- Command-line tooling lives in `mudu_cli`.
- Low-level system implementations live in `mudu_sys_impl` and `mudu_sys_wasm`.
- Test harnesses and integration testing helpers live in `testing` and `test_utils`.

## Main public entry points

- `json` — `read_json`, `write_json`, `json_value!` macro, and re-exported `JsonValue`/`JsonMap`/`JsonArray`/`JsonNumber` types.
- `toml` — `read_toml`, `write_toml`, `to_toml_str`.
- `md5` — `calc_md5`.
- `oid` — `gen_oid`, `new_xid`.
- `log` — `log_setup`, `log_setup_ex`.
- `notifier` — `Notifier`, `NotifyWait`, `Waiter`, `notify_wait`.
- `sync` — synchronization primitives re-exported from `mudu_sys`.
- `task_async` — async runtime helpers such as `spawn_task`, `spawn_local_task`, `block_on_tokio_current_thread`, `CurrentThreadTaskRuntime`, `timeout`, `sleep`, etc.
- `task_sync` — blocking thread helpers: `spawn_thread`, `spawn_thread_named`, `sleep_blocking`.
- `task_context` — per-task context storage.
- `task_id` — task identifier types and constructors.
- `task_trace` — `TaskTrace` / `NoopTaskTrace` and helpers like `dump_task_trace`, plus the `task_trace!` / `scoped_task_trace!` / `dump_task_trace!` / `task_backtrace!` macros.
- `thread_trace` — `ThreadTrace` / `NoopThreadTrace` and helpers like `dump_thread_trace`, plus the `thread_trace!` / `scoped_thread_trace!` / `dump_thread_trace!` / `thread_backtrace!` macros.
- `this_file` — `this_file!` macro for resolving the current source file path against the project home.
- `debug` — debug HTTP server registration and trace dumping (enabled by `debug_trace` feature).
