# mudu_compat_migrate

Pure migration framework for MuduDB format and protocol versions. 
It defines a registry of deterministic, side-effect-free handlers that transform binary payloads between versions, 
plans the shortest upgrade or rollback path across them, and exposes a global router that downstream crates can install once and use from anywhere.

## Responsibility

- Define the core abstractions for format/protocol migration: handlers, options, router, and errors.
- Plan and execute the shortest migration path between two supported versions.
- Provide a process-global compatibility router installed at startup.
- Keep migration logic I/O-free and deterministic: handlers are plain function pointers.

## What does NOT belong here

- Concrete migration handlers for a specific format — those live in the crate that owns the format's encode/decode logic, e.g. `mudu_kernel` for page header and log frame, `mudu_contract` for protocol frame and tuple binary.
- The definition of format kinds and version ranges — provided by the foundation `mudu` crate.
- Actual disk I/O or transaction management — handled by storage/execution crates.

## Main public entry points

- `error::MigrateError` — structured migration failures and conversion to `MuduError`.
- `handler::{MigrateHandler, MigrateOption, UpgradeFn, RollbackFn, clone_upgrade, clone_rollback}` — handler definition and identity helpers.
- `router::CompatibilityRouter` — version window configuration, handler registration, and migration execution/validation.
- `router::global` — process-global router installation and access.
- `router::{OptionProvider, NoopOptionProvider}` — pluggable auxiliary payload provider for migration steps.
