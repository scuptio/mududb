# worker

Worker execution layer: worker runtime, sessions, fs services, and config.

## Role

The execution core one level above `xcontract` in the server stack
(`server -> backup -> repl -> worker -> xcontract`, see
[ARCHITECTURE.md](../../../ARCHITECTURE.md)). Consumed by `server`, `repl`,
and `backup`; builds on `conn`, `sql`, `storage`, `xcontract`, `xengine`, and
`wal`.

## Key modules

- `worker` — the per-worker execution runtime (`WorkerRuntime`), shared by
  both the `io_uring` worker-ring loop and the Tokio worker loop so upper
  layers need no transport-specific worker abstraction. Split into focused
  submodules: `sql` (SQL text protocol and explicit transaction control),
  `kv` (session-scoped KV operations), `relation_ops` (relation DML),
  `procedure` (stored-procedure invocation), and `session_bound` (the
  session-bound runtime wrapper).
- `request_response_worker` — request/response API a worker exposes to
  connection handlers.
- `worker_session_manager` — connection-scoped session registry of a worker.
- `worker_registry` — persistent worker identities and topology lookup.
- `async_func_runtime` — async procedure invocation surface of a worker.
- `server_cfg` — server configuration types shared by the TCP backends.
- `routing` — session-to-worker routing configuration.
- `fs_service` / `fs_fd_table` / `fs_gc` — host-side fs object I/O service,
  the session-scoped file-descriptor tables, and fs object generation GC.
- `loop_stats` / `worker_loop_stats` — event-loop phase accounting and the
  counter snapshot of the `io_uring` worker event loop.

## Features

- `test` — test helpers and fixtures; may only be enabled by dev-dependencies,
  never by production dependencies (enforced by
  `script/ci/check_test_feature.py`).
- `debug_trace` — enables extra debug tracing via `mudu_utils/debug_trace`.
- `ds` — pass-through for `mudu_sys/ds` (deterministic-simulation backend; a
  no-op in this repository).
