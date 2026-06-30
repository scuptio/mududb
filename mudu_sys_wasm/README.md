# mudu_sys_wasm

Provides portable system-level utilities for WebAssembly targets, wrapping standard library, WASI, and external crates behind a small MuduDB-shaped API surface.

## Responsibility

- Expose a small, deterministic set of system services that compile and behave consistently in WebAssembly runtimes.
- Wrap `std::sync::Mutex` into `SMutex`, converting poisoning errors into MuduDB `ErrorCode::Mutex` results.
- Provide UUID / random value helpers (`uuid::Uuid` re-export and v4 generation).
- Provide time helpers (monotonic instant, system time, and `chrono` UTC) that rely on the host/WASI clock.

## What does NOT belong here

- WASM runtime hosting or module loading: that lives in `mudu_wasm` (or an equivalent runtime crate).
- General database logic, storage engines, or SQL parsing: use `mudu` or storage-specific crates.
- Platform-specific non-WASM system calls: prefer `mudu_sys` / OS-specific system crates.

## Main public entry points

- `mudu_sys_wasm::random` — `uuid_v4`, `next_uuid_v4_string`, and re-export of `uuid::Uuid`.
- `mudu_sys_wasm::sync` — `SMutex`, `SMutexGuard`.
- `mudu_sys_wasm::time` — `instant_now`, `system_time_now`, `utc_now`, plus re-exports of `Instant`, `SystemTime`, `DateTime<Utc>`.
