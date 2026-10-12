//! Kernel catalog: kernel-facing contracts, value types, shared id types,
//! and fuzzing helpers shared across the kernel subsystems.

#![warn(missing_docs)]
#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
#![deny(clippy::dbg_macro)]
#![warn(clippy::panic)]
#![warn(clippy::todo)]
#![warn(clippy::unimplemented)]

/// Shared helpers and newtype id macros.
pub mod common;
/// Kernel-facing contracts and value types exchanged between subsystems.
pub mod contract;
/// Frame-decoding helpers.
pub mod frame;
/// Kernel-side replication configuration types.
pub mod repl_cfg;
/// Replication-control interfaces exposed to the worker/transaction layers.
pub mod repl_ctrl;
/// Per-stage timing instrumentation shared across the kernel.
pub mod stage_stats;
