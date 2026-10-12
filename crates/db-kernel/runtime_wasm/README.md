# runtime_wasm

The Mudu procedure host: WebAssembly component loading and instantiation
(Wasmtime), guest/host interface bindings, app package management, and schema
resolution.

## Role

Recently split out of `mudu_runtime`; the pgwire/HTTP serving backend stays in
`mudu_runtime` and depends on this crate (as does `testing`). Builds on
`server`, `conn`, `runtime_libsql`, `xengine`, and `wasmtime` /
`wasmtime-wasi`.

## Key modules

- `service` — core runtime services for loading, installing, and invoking
  Mudu packages: `.mpk` app-package loading, app instance management, the
  procedure instance pool, and the Wasmtime runtime wrappers.
- `interface` — host-side interface functions exposed to guest modules
  (synchronous and asynchronous kernel calls).
- `procedure` — a loaded procedure: its descriptor paired with the
  pre-instantiated component that procedure instances are leased from.
- `resolver` — schema resolution and management for Mudu applications.

## Test fixtures

`data/wasm_module/` holds the prebuilt `app1.mpk` guest package used by the
component/procedure tests, together with its package descriptor and DDL. See
[data/wasm_module/README.md](data/wasm_module/README.md) for how to rebuild
it from `example/mpk/app1`.

## Features

- `debug_trace` — enables extra debug tracing via `mudu_utils/debug_trace`.
- `ds` — pass-through for `mudu_sys/ds` (deterministic-simulation backend; a
  no-op in this repository). Tests that exercise real-OS semantics the
  simulator cannot provide are gated behind `#[cfg(not(feature = "ds"))]`.
