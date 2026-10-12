# MuduDB Workspace Architecture

This document describes the crate layout, dependency direction, and key design decisions for the `mududb` workspace.

## Repository Layout

The repository root is **not** a cargo workspace. Crates live under `crates/`
in group workspaces, each with its own `Cargo.toml` and `Cargo.lock`:

```text
crates/
├── common/      # foundation, sys abstraction, types, bindings, parser, client
├── db-kernel/   # kernel, runtime, raft consensus (mudu_raft), server (mudud), adapter, testing
├── sdk/         # mududb SDK, bindings (assemblyscript, python), APIs
├── tools/       # mudu_cli, mudu_gen, mudu_transpiler, mpm_build,
│                # mpm_crate, mpm_install
└── bindings/    # standalone cdylib workspace: mudu_adapter_ffi (native adapter
                 # ABI); NOT part of any group workspace, isolated target-dir
example/
└── mpk/         # separate consumer workspace: guest-SDK examples (wallet, tpcc, ...)
```

The five group/example workspaces are `crates/{common,db-kernel,sdk,tools}` and
`example/mpk`. The guest-SDK examples were split out of `crates/sdk` into
their own workspace (now the top-level `example/mpk`) so that the SDK library
workspace no longer compiles guest wasm crates (and their kernel-scale
dev-dependencies) on every `cargo test --workspace`.

Cross-group path dependencies (e.g. `crates/db-kernel/*` depending on
`crates/common/mudu`) are legal and used heavily; cargo builds them as normal
path dependencies of the consuming workspace. All workspaces share the
repository-root `target/` directory (root `.cargo/config.toml` sets
`[build] target-dir = "target"`), so build artifacts like
`target/wasm32-wasip2/release/*.mpk` land at the repository root regardless
of which workspace produced them. Run cargo commands per
workspace (`cd crates/<group> && cargo ...`); `script/shell/check_all.sh`
orchestrates fmt/clippy/test-compile across all five.

## Crate Layers

```text
┌─────────────────────────────────────────────────────────────┐
│  Apps / Examples / CLI / Tests                               │
│  (mudud, mudu_cli, mpm_build, mpm_install, testing)          │
├─────────────────────────────────────────────────────────────┤
│  Server                                                      │
│  (server -> backup -> repl ->           │
│   worker -> xcontract)                         │
├─────────────────────────────────────────────────────────────┤
│  Kernel                                                      │
│  (conn -> sql -> command -> meta │
│   -> storage -> xengine -> wal ->       │
│   index -> catalog)                            │
├─────────────────────────────────────────────────────────────┤
│  Runtime & Raft                                              │
│  (mudu_runtime, mudu_raft, sys_interface_standalone)         │
├─────────────────────────────────────────────────────────────┤
│  Language / Binding / Contract                               │
│  (mudu_contract, mudu_binding, mudu_type, mudu_transpiler,  │
│   sql_parser, mudu_gen, mudu_client)                        │
├─────────────────────────────────────────────────────────────┤
│  Utilities                                                   │
│  (mudu_utils, mudu_build_common)                            │
├─────────────────────────────────────────────────────────────┤
│  System Abstraction                                          │
│  (mudu_sys -> mudu_sys_impl -> mudu_sys_contract)           │
├─────────────────────────────────────────────────────────────┤
│  Foundation                                                  │
│  (mudu: result types, error codes, macros, pure utilities)  │
└─────────────────────────────────────────────────────────────┘
```

The original `mudu_kernel` monolith (76.9k lines) was split into fourteen
kernel crates plus a thin `mudu_kernel` facade. `mudu_kernel` now only
re-exports the kernel crates (and keeps `compat`, the fuzz harness, and
test utilities), preserving the historical `mudu_kernel::server`,
`mudu_kernel::storage`, etc. module paths for `mudu_runtime`, `mudu_cli`, and
`testing`.

## Key Rules

1. **`mudu` is a pure foundation crate.**
   - It provides common types (`RS`), error codes (`EC`), the `mudu_error!` macro, and pure serialization helpers.
   - It must **not** perform I/O, read environment variables, or depend on `mudu_sys`.
   - File I/O helpers (e.g. `read_json`, `write_toml`) live in `mudu_utils`.

2. **`mudu_sys_impl` is the single place for OS / async abstractions.**
   - Wrappers for `std::fs`, `std::thread`, `std::net`, `tokio`, etc.
   - It is re-exported by `mudu_sys`.
   - All other crates use `mudu_sys::*` instead of raw `std` / `tokio` APIs.

3. **No crate outside `mudu_sys_impl` may use disallowed `std`/`tokio` APIs.**
   - The workspace clippy configuration enforces this via `disallowed_methods` and `disallowed_types`.

4. **Build scripts share logic through `mudu_build_common`.**
   - Path resolution, file-copy-on-change, workspace version extraction, and tree-sitter constant generation are centralized there.

5. **Integration tests share helpers through `testing::support`.**
   - Common boilerplate such as `supports_server_mode`, `temp_dir`, `TestListener`, and `wait_until_backend_ready` should be reused rather than duplicated.

## Dependency Notes

- `mudu_sys_impl` depends on `mudu` for `RS`/`EC`/`mudu_error!`. 
- `mudu_utils` depends on both `mudu` and `mudu_sys`, making it the natural home for higher-level I/O utilities.
- `sql_parser` parses DDL into `TableDef` and also hosts the SQL static-check core (`sql_parser::check`) used by `mgen check-sql`.
- `mududb.ds` is a separate workspace that consumes the public crates from `mududb` for deterministic simulation / model checking.

### Dependency consistency and layering enforcement

- `versions.toml` (repo root) is the single source of truth for external
  dependency specs shared across two or more group workspaces (including the
  `example/mpk` consumer workspace). `script/ci/sync_workspace_deps.py`
  rewrites the matching lines in the `crates/*/Cargo.toml`
  `[workspace.dependencies]` blocks; its `--check` mode fails on drift and also
  requires every shared dependency to be listed (and every listed dependency
  to be shared).
- `script/ci/check_arch.py` enforces the crate layering: `common` may not
  depend on `db-kernel`/`sdk`/`tools`, `db-kernel` may not depend on
  `sdk`/`tools`, and `sdk`/`tools` may not depend on each other. The pure
  `mudu` crate must depend on nothing. The `example/mpk` crates are treated as
  part of the `sdk` layer. Both scripts run in `script/shell/check_all.sh` and
  the `workspace-invariants` CI job.
- `crates/bindings/mudu_adapter_ffi` is a standalone cdylib workspace (native
  adapter ABI). It is deliberately excluded from the group/example workspaces
  and redirects its build artifacts to its own `target-ffi/` so its unhashed
  `libmudu_adapter_ffi.so` never lands in the shared repository-root `target/`.

