# crates/

All MuduDB Rust crates, grouped into independent cargo workspaces. See
[ARCHITECTURE.md](../ARCHITECTURE.md) for the crate layering and dependency
rules, and [AGENTS.md](../AGENTS.md) for the contributor checklist.

## Groups

- **[common](common/)** — foundation layer: the pure `mudu` crate, the
  `mudu_sys*` system-abstraction stack, data types (`mudu_type`), the
  contract and wire-protocol client (`mudu_contract`, `mudu_client`), the
  language-binding codecs (`mudu_binding`), and the SQL parser.
- **[db-kernel](db-kernel/)** — the database kernel, 23 crates: a thin
  `mudu_kernel` facade plus the fourteen short-named crates split out of
  the former monolith (`catalog`, `index`, `wal`, `xengine`, `storage`,
  `meta`, `command`, `sql`, `conn`, `xcontract`, `worker`, `repl`,
  `backup`, `server`), the serving runtimes (`mudu_runtime`,
  `runtime_wasm`, `runtime_libsql`), the `mudud` server daemon, the
  `mudu_raft` consensus library (a vendored port of scuptio/scupt-raft),
  `mudu_adapter`, `sys_interface_standalone`, and `testing`.
- **[sdk](sdk/)** — the `mududb` guest SDK and `mudu_api*` crates, and the
  language bindings (`bindings/` for AssemblyScript, C, Go, and Python;
  `mudu_api/` for C# and Rust). The guest-SDK example applications
  (wallet, vote, tpcc, ycsb, ...) live in the separate top-level consumer
  workspace [`example/mpk`](../example/mpk/).
- **[tools](tools/)** — developer tooling: `mudu_gen` (mgen, code
  generator), `mudu_transpiler` (mtp), the `mpm_*` package tools
  (`mpm_build`, `mpm_crate`, `mpm_install`), and the `mudu_cli` (mcli)
  client.
- **[bindings](bindings/)** — `mudu_adapter_ffi`, a standalone cdylib
  workspace exporting the native adapter ABI, deliberately isolated from
  the shared `target/` directory (see [its README](bindings/README.md)).

## Workspace rules

- The repository root is **not** a cargo workspace. Each group is an
  independent workspace with its own `Cargo.toml` and `Cargo.lock`.
- All group workspaces share the repository-root `target/` directory: the
  root `.cargo/config.toml` sets `[build] target-dir = "target"`.
- Cross-group path dependencies use the `../../<group>/<crate>` form.
- Run cargo commands from inside a group directory, e.g.
  `cd crates/db-kernel && cargo test --workspace`.
- `bash script/shell/check_all.sh` runs fmt + clippy + test compilation
  across all group/example workspaces.
