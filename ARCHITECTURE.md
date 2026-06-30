# MuduDB Workspace Architecture

This document describes the crate layout, dependency direction, and key design decisions for the `mududb` workspace.

## Crate Layers

```text
┌─────────────────────────────────────────────────────────────┐
│  Apps / Examples / CLI / Tests                               │
│  (mudud, mudu_cli, mudu_package, testing, example/*)        │
├─────────────────────────────────────────────────────────────┤
│  Runtime & Kernel                                            │
│  (mudu_runtime, mudu_kernel)                                │
├─────────────────────────────────────────────────────────────┤
│  Language / Binding / Contract                               │
│  (mudu_contract, mudu_binding, mudu_type, mudu_transpiler,  │
│   sql_parser, mudu_gen)                                     │
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

## Key Rules

1. **`mudu` is a pure foundation crate.**
   - It provides common types (`RS`), error codes (`EC`), the `m_error!` macro, and pure serialization helpers.
   - It must **not** perform I/O, read environment variables, or depend on `mudu_sys`.
   - File I/O helpers (e.g. `read_json`, `write_toml`) live in `mudu_utils`.

2. **`mudu_sys_impl` is the single place for OS / async abstractions.**
   - Wrappers for `std::fs`, `std::thread`, `std::net`, `tokio`, etc.
   - It is re-exported by `mudu_sys`.
   - All other crates use `mudu_sys::*` instead of raw `std` / `tokio` APIs.

3. **No crate outside `mudu_sys_impl` may use disallowed `std`/`tokio` APIs.**
   - The workspace clippy configuration enforces this via `disallowed-methods` and `disallowed-types`.

4. **Build scripts share logic through `mudu_build_common`.**
   - Path resolution, file-copy-on-change, workspace version extraction, and tree-sitter constant generation are centralized there.

5. **Integration tests share helpers through `testing::support`.**
   - Common boilerplate such as `supports_server_mode`, `temp_dir`, `TestListener`, and `wait_until_backend_ready` should be reused rather than duplicated.

## Dependency Notes

- `mudu_sys_impl` depends on `mudu` for `RS`/`EC`/`m_error!`. 
- `mudu_utils` depends on both `mudu` and `mudu_sys`, making it the natural home for higher-level I/O utilities.
- `mududb.ds` is a separate workspace that consumes the public crates from `mududb` for deterministic simulation / model checking.
