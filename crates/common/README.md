# crates/common

The foundation group of the MuduDB workspace: the pure `mudu` base crate,
the `mudu_sys*` system-abstraction stack, shared types and contracts, the
SQL parser, the wire-protocol client, and the tree-sitter grammar crates.
Per the layering rules in [ARCHITECTURE.md](../../ARCHITECTURE.md), crates
in this group may not depend on `db-kernel`, `sdk`, or `tools`. See the
root [AGENTS.md](../../AGENTS.md) for build and lint conventions.

## Crates

| Crate                                       | Purpose |
| ------------------------------------------- | ------- |
| [mudu](mudu/)                               | Pure foundation crate: common types, error codes, macros, and serialization helpers. Performs no I/O and does not depend on `mudu_sys`. |
| [mudu_binding](mudu_binding/)               | Bindings and serialization layer between `mudu` core types and their portable/universal representations for FFI, RPC, and storage. |
| [mudu_build_common](mudu_build_common/)     | Shared build-script helpers for the workspace (path resolution, copy-if-changed, version parsing, constant generation). |
| [mudu_client](mudu_client/)                 | TCP/JSON wire-protocol client and management HTTP API library for MuduDB. |
| [mudu_compat_migrate](mudu_compat_migrate/) | Pure, I/O-free migration framework for MuduDB format and protocol versions. |
| [mudu_contract](mudu_contract/)             | Core contract types and protocols shared between client and server: database abstraction, procedure descriptors, wire protocol frames, tuple encoding. |
| [mudu_sys](mudu_sys/)                       | Target-selecting facade for the MuduDB system-services API. |
| [mudu_sys_contract](mudu_sys_contract/)     | System-level async I/O contracts and performance tracing primitives behind `mudu_sys` implementations. |
| [mudu_sys_impl](mudu_sys_impl/)             | Native implementation of the system interface; the single home of the OS/async wrappers (filesystem, networking, threads, tokio). |
| [mudu_sys_wasm](mudu_sys_wasm/)             | Minimal system services available to WebAssembly applications. |
| [mudu_type](mudu_type/)                     | The mududb data type system: type descriptors, runtime values, and the per-type conversion, comparison, and parameter function tables. |
| [mudu_utils](mudu_utils/)                   | I/O-performing utility facade built on `mudu` and `mudu_sys`. |
| [sql_parser](sql_parser/)                   | SQL parser: statement ASTs, DDL parsing into `TableDef`, and the static-check core used by `mgen check-sql`. |
| [sys_interface](sys_interface/)             | Synchronous and asynchronous bindings/adapters over the host system interface, including component-model bindings for WebAssembly. |
| [test_utils](test_utils/)                   | Arbitrary / QuickCheck utilities for tests. |
| [tree-sitter-sql](tree-sitter-sql/)         | C grammar bindings crate: the tree-sitter SQL grammar. |
| [tree-sitter-wit](tree-sitter-wit/)         | C grammar bindings crate: the tree-sitter WIT grammar. |
