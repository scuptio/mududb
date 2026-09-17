//! Pure migration framework for MuduDB format and protocol versions.
//!
//! This crate is intentionally I/O-free: all migration functions are plain
//! function pointers and must be deterministic and side-effect free. It depends
//! only on the foundation `mudu` crate for [`FormatKind`] and error types.
//!
//! Concrete migration handlers for each format live in the crate that owns the
//! format's encode/decode implementation (e.g. `mudu_kernel` for page header
//! and log frame, `mudu_contract` for protocol frame and tuple binary).
//!
//! [`FormatKind`]: mudu::compat::FormatKind

#![warn(missing_docs)]

pub mod error;
pub mod handler;
pub mod router;

#[cfg(test)]
mod tests;

pub use error::MigrateError;
pub use handler::{
    clone_rollback, clone_upgrade, MigrateHandler, MigrateOption, RollbackFn, UpgradeFn,
};
pub use router::{global, CompatibilityRouter, NoopOptionProvider, OptionProvider};
