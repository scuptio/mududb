# mudu_build_common

Shared build-script helpers for the MuduDB workspace, providing utilities for
incremental code generation, manifest generation, and tree-sitter constant
extraction from `grammar.json` files.

## Responsibility

- Detecting the repository root and emitting `cargo:rerun-if-changed` directives.
- Reading selected dependency versions from the workspace `Cargo.toml`.
- Incremental file operations: copy, write, and stale-file removal that only
  mutate disk when content changes.
- Collecting Rust source files and generating a `mod.rs` that declares them.
- Generating `Cargo.toml` content for the Rust SDK (`mudu_api_rust`) and its demo.
- Generating Rust constants (node kind IDs/names, field IDs/names, and sequence
  indices) from a tree-sitter `grammar.json`.

## What does NOT belong here

- Concrete SDK binding generation logic lives in `mudu_api_sync` and the
  language-specific binding crates such as `mudu_binding`.
- The actual tree-sitter grammars and parser code live in `tree-sitter-sql` and
  `tree-sitter-wit`.
- Low-level filesystem and environment abstractions live in `mudu_sys`.
- The core database implementation lives in `mudu` and `mududb`.

## Main public entry points

- `mudu_build_common::Result<T>` — result alias for fallible build helpers.
- `repo_root()` — resolves the workspace repository root.
- `rerun_if_changed()` — emits a Cargo rerun directive.
- `read_workspace_versions()` — extracts dependency versions from the workspace
  manifest.
- `copy_file_if_changed()` / `write_if_changed()` — incremental file writes.
- `remove_stale_files()` — deletes files in a directory that are no longer needed.
- `collect_universal_files()` / `generate_universal_mod()` — helpers for generating
  module declarations.
- `generate_sdk_manifest()` / `generate_demo_manifest()` — generates `Cargo.toml`
  content for the Rust SDK.
- `ts_const_generate()` — generates tree-sitter constant Rust files from a grammar.
