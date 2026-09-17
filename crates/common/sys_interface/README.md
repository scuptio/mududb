# sys_interface

Bindings and adapters for the Mudu database system interface. This crate exposes synchronous and asynchronous APIs over the host system interface, including component-model bindings for WebAssembly.

## Responsibility

- Expose the platform-specific top-level Mudu system API.
- Provide synchronous and asynchronous wrappers over host system operations.
- Serialize and invoke host system operations via `host` helpers.
- Supply WebAssembly component-model bindings when targeting `wasm32`.

## What does NOT belong here

- Core database engine logic and query execution (lives in `mudu`).
- Schema, contract, and type definitions (lives in `mudu_contract` / `mudu_type`).
- Low-level system interface implementation details (lives in `mudu_sys`).
- General binding generation utilities (lives in `mudu_binding`).
- The adapter-backed standalone implementation and its UniFFI bindings (lives in `sys_interface_standalone`).
- Standalone adapter runtime and deployment concerns (lives in `mudu_adapter`).

## Main public entry points

- `api` — Re-exported platform-specific top-level API.
- `sync_api` — Synchronous top-level API.
- `async_api` — Asynchronous top-level API.
- `host` — Helpers for serializing and invoking host system operations.

For local standalone syscall execution through the in-process adapter (and the
optional UniFFI bindings), use the `sys_interface_standalone` crate — directly,
or via the `standalone-adapter` feature of the `mududb` facade.
