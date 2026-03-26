# `mudu_api_sync`

`mudu_api_sync` is a workspace-only helper crate that keeps the standalone Rust SDK under `mudu_api/rust` synchronized with the source-of-truth files in the main repository.

## What it does

During a normal workspace build, the crate's `build.rs` automatically:

- copies every Rust source file from `mudu_binding/src/universal/` into `mudu_api/rust/src/universal/`
- regenerates `mudu_api/rust/src/universal/mod.rs`
- copies `sys_interface/wit/async/async-api.wit` into `mudu_api/rust/wit/async-api.wit`
- rewrites `mudu_api/rust/Cargo.toml`
- rewrites `mudu_api/rust/demo/Cargo.toml`

The generated SDK remains self-contained and can still be copied or distributed independently of the repository workspace.

## Why this exists

The Rust SDK must satisfy two constraints at the same time:

- it must be distributable as a standalone package
- it must stay aligned with the workspace definitions and dependency versions

This crate provides the bridge between those two requirements.

## Trigger

Because `mudu_api_sync` is a workspace member, a root-level command such as:

```bash
cargo build
```

will build this crate and run its synchronization step automatically.

You can also run it directly:

```bash
cargo build -p mudu_api_sync
```

## Notes

- `mudu_api/rust/Cargo.toml` and `mudu_api/rust/demo/Cargo.toml` are generated files.
- `mudu_api/rust/src/universal/mod.rs` is generated even though the source directory also contains a `mod.rs` in the workspace version.
- If a file is removed from `mudu_binding/src/universal/`, the stale copy is removed from `mudu_api/rust/src/universal/` on the next sync.
