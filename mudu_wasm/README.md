# mudu_wasm

WebAssembly bindings for MuduDB. This crate builds as a `cdylib` and exposes
MuduDB procedures through the WebAssembly component model: on `wasm32` targets
it provides the generated guest bindings, while on `x86_64` it provides the
transpilation helpers used to produce those bindings.

> **Note on crate name:** although the directory and human-readable name are
> `mudu_wasm`, the package is published under the Cargo crate name `mod_0` (see
> `Cargo.toml`). Therefore public Rust paths begin with `mod_0::`, and the
> component module name used by the runtime is also `mod_0`.

## Responsibility

- Provide a `cdylib` WebAssembly target for MuduDB.
- Expose generated component-model bindings for `wasm32` (behind the `transpile`
  feature).
- Provide x86_64 transpilation helpers that generate/maintain the WASM
  component bindings.
- Bridge MuduDB procedures (`proc`, `proc2`) and their descriptors to the WASM
  component model.

## What does NOT belong here

- Core MuduDB engine and component-model implementation live in `mududb`.
- Procedure contract and descriptor definitions live in `mudu_contract` and
  `mudu_sys_contract`.
- Host-side system call implementation belongs in `mudu_sys_impl` and
  `mudu_sys_wasm`.
- Non-WebAssembly language bindings belong in `mudu_binding`.
- Transpiler core logic belongs in `mudu_transpiler`.
- CLI tooling belongs in `mudu_cli`.
- WIT/tree-sitter parsing belongs in `tree-sitter-wit`.

## Main public entry points

- `mod_0::generated` — generated WebAssembly component bindings, available
  on `wasm32` with the `transpile` feature.
  - `generated::proc` — `proc_mtp` and `mudu_*_desc_proc_mtp` descriptor helpers.
  - `generated::proc2` — `proc2_mtp`, `proc_sys_call_mtp`, and related
    descriptor helpers.
- `mod_0::wasm_mtp` — x86_64 transpilation helpers.
  - `wasm_mtp::proc` — host-side `proc_mtp` helper.
  - `wasm_mtp::proc2` — host-side `proc2_mtp` and `proc_sys_call_mtp` helpers.

There are no binaries; the crate is consumed as a library / `cdylib`.
