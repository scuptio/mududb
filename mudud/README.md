# mudud

`mudud` is the MuduDB server daemon binary. It bootstraps a database server process by parsing command-line arguments, initializing logging, loading the runtime configuration, and driving the backend until a graceful shutdown signal is received.

## Responsibility

- Parse command-line arguments for the server (`--cfg`).
- Initialize the logging/tracing subsystem.
- Load the MuduDB runtime configuration (`mududb_cfg`).
- Spawn a signal-listener thread for graceful shutdown.
- Drive the runtime backend (`Backend::sync_serve_with_stop`).
- Coordinate shutdown between the backend and the signal listener.

## What does NOT belong here

- The database runtime/execution engine implementation — see `mudu_runtime`.
- Core error types, result aliases, and common macros — see `mudu`.
- OS-level task/thread primitives and shutdown signal handling — see `mudu_sys`.
- Logging setup, notification primitives, and other shared utilities — see `mudu_utils`.
- Interactive client tooling — see `mudu_cli`.
- SQL parsing — see `sql_parser`.
- Storage kernel internals — see `mudu_kernel`.
- Wasm/contract execution — see `mudu_wasm` / `mudu_sys_wasm`.

## Main public entry points

- The `mudud` executable binary.
- CLI option `--cfg <FILE>` to specify the path to the MuduDB configuration TOML file.
