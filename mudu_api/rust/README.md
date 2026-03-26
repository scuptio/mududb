# Mudu API Rust Library

This crate mirrors the shape of `mudu_api/csharp`, but exposes an async Rust API.
It is intentionally self-contained so it can be copy/pasted or distributed on its own.

It provides:

- `Mudu::command(...)`
- `Mudu::query(...)`
- `mudu_sys` raw async syscall helpers
- a SQLite mock backend behind the `mock-sqlite` feature
- vendored `universal` models under `src/universal/`
- a runnable demo under `demo/`

## Standalone Layout

- `Cargo.toml` is the local workspace root for the Rust SDK
- `demo/` is a workspace member of the SDK, not of the repository root
- the SDK does not depend on repository workspace crates

## Features

- `mock-sqlite`: enable the SQLite-backed mock backend
- `wasm-async`: enable wasm async WIT imports for `wasm32`

## Backend selection

Backend resolution is simple:

1. If `mock-sqlite` is enabled, the mock backend is used.
2. Otherwise, when compiling for `wasm32` with `wasm-async`, the async WIT imports are used.
3. Otherwise, calls fail with a backend-unavailable error.

## Demo

Run the mock-backed demo with:

```bash
cargo run -p mudu_api_rust_demo
```
