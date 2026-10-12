# db-kernel

The database kernel group. The original `mudu_kernel` monolith was split into
a thin `mudu_kernel` facade plus a set of layered, short-named kernel crates
(`catalog`, `index`, `wal`, `xengine`, `storage`, `meta`, `command`, `sql`,
`conn` on the engine path, and `xcontract`, `worker`, `repl`, `backup`,
`server` on the server path). Around them sit the runtime trio —
`mudu_runtime` (pgwire/HTTP serving), `runtime_wasm` (WebAssembly procedure
host), and `runtime_libsql` (libsql connector) — the `mudud` server daemon,
the `mudu_raft` consensus library, the `mudu_adapter` backend router,
`sys_interface_standalone`, and the `testing` integration-test helpers.

## Crates

| Crate                      | Purpose                                                                                                      |
|----------------------------|--------------------------------------------------------------------------------------------------------------|
| `backup`                   | WAL archiving, base backup/PITR restore, and logical dump/restore.                                           |
| `catalog`                  | Kernel-facing contracts, value types, shared id types, and fuzzing helpers.                                  |
| `command`                  | Command interpreters for DDL and DML.                                                                        |
| `conn`                     | Async connection, prepared-statement, and result-set wrappers over the kernel engine.                        |
| `index`                    | B-tree index implementation and key encodings used by the execution layer.                                   |
| `meta`                     | In-memory catalog managers for schemas, partitions, rules, and placements.                                   |
| `mudu_adapter`             | Adapter routing storage operations to SQLite, PostgreSQL, MySQL, or the remote Mudud protocol.               |
| `mudu_kernel`              | Thin facade re-exporting the kernel crates; preserves the historical `mudu_kernel::*` paths.                 |
| `mudu_raft`                | Raft consensus for replication; a vendored port of `scuptio/scupt-raft` restructured onto `mudu`/`mudu_sys`. |
| `mudu_runtime`             | Serving runtime: pgwire/HTTP session handling and process management.                                        |
| `mudud`                    | Server daemon binary and its testable library: config loading, argument parsing, serve logic.                |
| `repl`                     | Primary-replica streaming replication with Raft fault tolerance.                                             |
| `runtime_libsql`           | libsql-based database connector: connection, transaction, and result-set drivers.                            |
| `runtime_wasm`             | WebAssembly procedure host: component loading, guest/host interface, app package management.                 |
| `server`                   | TCP server backend with a Linux-first `io_uring` implementation.                                             |
| `sql`                      | SQL parsing, binding, planning, and statement execution.                                                     |
| `storage`                  | On-disk storage layer: pages, relations, and time-series files.                                              |
| `sys_interface_standalone` | Adapter-backed `sys_interface` implementation for native targets (local tests and debug runs).               |
| `testing`                  | Integration-test helpers for the MuduDB workspace.                                                           |
| `wal`                      | Write-ahead log format, serialization, and per-worker backend.                                               |
| `worker`                   | Worker execution layer: worker runtime, sessions, fs services, and config.                                   |
| `xcontract`                | Kernel contract prelude: the worker-side execution contract and the shared types kernel crates consume.      |
| `xengine`                  | Public cross-engine API and the main `MuduEngine` implementation.                                            |

## Layering

The dependency layering of these crates is defined in
[ARCHITECTURE.md](../../ARCHITECTURE.md) and enforced by
`script/ci/check_arch.py`, which runs in `script/shell/check_all.sh` and in
the `workspace-invariants` CI job. In short: `common` may not depend on
`db-kernel`/`sdk`/`tools`, and `db-kernel` may not depend on `sdk`/`tools`.

## Building and testing

This directory is its own cargo workspace; run cargo commands from here:

```bash
cd crates/db-kernel
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```
