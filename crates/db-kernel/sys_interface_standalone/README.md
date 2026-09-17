# sys_interface_standalone

Standalone adapter-backed implementation of the Mudu system interface. This crate routes the `sys_interface` syscall APIs to the in-process `mudu_adapter` backend on native (non-`wasm32`) targets, so user code can run against SQLite / PostgreSQL / MySQL / a remote `mudud` without the kernel/runtime stack.

## Responsibility

- Provide adapter-backed synchronous (`sync_api`) and asynchronous (`async_api`) implementations of the `sys_interface` syscall surface.
- Provide the byte-level session entry points (`mudu_open_bytes`, `mudu_close_bytes`, `mudu_get_bytes`, `mudu_put_bytes`, `mudu_range_bytes`) on top of the adapter-backed typed functions; re-export the backend-independent byte entry points (`mudu_query_bytes`, `mudu_fetch_bytes`, `mudu_command_bytes`, `mudu_batch_bytes`) from `sys_interface`.
- Re-export `sys_interface::fs` and `sys_interface::host` so the module layout matches `sys_interface`.
- Optionally expose UniFFI foreign-function bindings for mobile/native interop (requires the `uniffi-bindings` feature).

## What does NOT belong here

- Backend-independent syscall glue (byte-level query/command/fetch/batch, wasm component bindings): lives in `sys_interface`.
- The adapter itself (drivers, session management, fs emulation): lives in `mudu_adapter`.
- Core database engine logic and query execution: lives in `mudu` / `mudu_kernel`.

## Main public entry points

- `api` — Re-exported top-level API (sync or async, selected by the `async` feature).
- `sync_api` — Synchronous adapter-backed top-level API.
- `async_api` — Asynchronous adapter-backed top-level API.
- `fs` / `host` — Re-exports from `sys_interface`.
- `uniffi` — Optional UniFFI foreign-function bindings (requires the `uniffi-bindings` feature).

See `standalone_adapter.md` for usage and connection configuration (`MUDU_CONNECTION`).
