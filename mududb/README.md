# mududb

`mududb` is the app-facing facade crate for MuduDB. It aggregates and re-exports the public interfaces that Mudu application code needs — core types, contract helpers, bindings, system calls, and error types — behind a small set of feature flags, so apps can depend on one crate rather than many internal implementation crates.

## Responsibility

- Provide the curated public API surface used by Mudu apps.
- Re-export the core `mudu` crate and selected submodules (`common`, `error`, `mudu_error`).
- Re-export domain crates as flat modules: `types` (`mudu_type`), `contract` (`mudu_contract`), `binding` (`mudu_binding`), and `sys` (`mudu_sys`).
- Expose the convenience macros `sql_params!` and `sql_stmt!` from `mudu_contract`.
- Gate advanced/low-level access with Cargo features such as `interface`, `component-model`, `wasip2`, `async`, `standalone-adapter`, and `uniffi-bindings`.

## What does NOT belong here

- Core runtime, storage engine, and kernel implementation: `mudu`, `mudu_runtime`, `mudu_kernel`.
- Low-level syscall ABI definitions and host implementation: `sys_interface`, `mudu_sys`, `mudu_sys_impl`, `mudu_sys_wasm`, `mudu_adapter`.
- Concrete domain implementations that are only re-exported here: `mudu_type`, `mudu_contract`, `mudu_binding`.
- Code generation, SQL parsing, and grammar crates: `mudu_gen`, `mudu_transpiler`, `sql_parser`, `tree-sitter-sql`, `tree-sitter-wit`.
- CLI, packaging, build tooling, and example applications: `mudu_cli`, `mudu_package`, `mudu_build_common`, `example/*`.

## Main public entry points

- `mududb::mudu` — re-export of the core `mudu` crate.
- `mududb::common` — common utilities from `mudu::common`.
- `mududb::error` / `mududb::mudu_error` — error types from `mudu`.
- `mududb::types` — public types from `mudu_type`.
- `mududb::contract` — contract/schema helpers from `mudu_contract`, including the `sql_params!` and `sql_stmt!` macros.
- `mududb::binding` — binding support from `mudu_binding`.
- `mududb::sys` — system-call exports from `mudu_sys`.
- `mududb::sys_interface` (requires the `interface` feature) — low-level syscall interface re-export.
