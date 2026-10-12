# server

TCP server backend with a Linux-first `io_uring` implementation.

On Linux the backend uses the native `io_uring` worker loop; on other
platforms the same public API falls back to a portable Tokio TCP
implementation, so modules that depend on `rliburing` are compiled only on
Linux.

## Role

Top of the server stack in `crates/db-kernel` (`server -> backup -> repl ->
worker -> xcontract`, see [ARCHITECTURE.md](../../../ARCHITECTURE.md)).
Consumed by `mudu_runtime`, `runtime_wasm`, and the `mudu_kernel` facade.
`lib.rs` re-exports the worker-layer surface (`worker`, `worker_session_manager`,
`fs_service`, `server_cfg`, ...), the backup modules, `repl`, and the
`xcontract` prelude so downstream crates keep importing them from their
historical `server::*` paths.

## Key modules

- `server` — the server backends proper: the Linux `io_uring` worker loop and
  the portable Tokio TCP worker loop with their supporting drivers
  (`message_bus` worker-local transport, `serve` startup, `worker_loop`
  connection loop, `drivers` WAL recovery/flush tasks, `repl_setup`
  replication wiring).
- `server_launch` — one-shot server start request carrying the configuration,
  the assembled runtime dependencies, and any pre-bound listeners.
- `server_runtime_deps` — runtime dependencies assembled from the server
  configuration before the workers start.
- `app_schema_lookup` — application schema lookup for pre-flight parameter
  type checking (the runtime registers a provider here so the protocol
  handlers can query it without a dependency cycle).
- `connection_state` — client connection lifecycle states.

## Features

- `debug_trace` — enables extra debug tracing via `mudu_utils/debug_trace`.
- `ds` — pass-through for `mudu_sys/ds` (deterministic-simulation backend; a
  no-op in this repository).
