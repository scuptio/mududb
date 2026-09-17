# mudu_sys

`mudu_sys` is a target-selecting facade crate for the MuduDB workspace. It exposes a single system-interface API to the rest of the project, routing at compile time to a concrete native implementation on non-WASM targets and a minimal WASM implementation on `wasm32`.

## Responsibility

- Provide a unified system API for filesystem, networking, process, threading, synchronization, time, and task runtime concerns.
- Select the appropriate backend at compile time (`mudu_sys_impl` for native targets, `mudu_sys_wasm` for `wasm32`).
- Allow downstream crates to depend on one crate instead of conditionally selecting a system backend.

## What does NOT belong here

- Concrete native OS/IO abstractions: implemented in `mudu_sys_impl`.
- WASM-specific shims and portable services: implemented in `mudu_sys_wasm`.
- System interface contracts and traits: defined in `mudu_sys_contract`.
- Core database types, contracts, and business logic: see `mudu`, `mudu_type`, `mudu_contract`, and `mududb`.

## Main public entry points

- `pub use mudu_sys_impl::*` on native targets, re-exporting modules such as `common`, `contract`, `env`, `env_var`, `fs`, `imp`, `io`, `net`, `process`, `provider`, `random`, `sync`, `sys_io_context`, `task`, `time`, and `server`.
- `pub use mudu_sys_wasm::*` on `wasm32`, re-exporting `random`, `sync`, and `time`.
- Key re-exported types (native): `Sys`, `SysIoContext`, `TaskID`, `TaskContext`, `TaskJoinHandle`, and subsystem handles (`SysEnvVar`, `SysOs`, `SysProcess`, `SysRandom`, `SysSync`, `SysTasks`, `SysThread`, `SysTime`).
- Cargo features: `native` (default), `debug_trace`.
