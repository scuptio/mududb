#![allow(missing_docs)]

//! Error types and codes used by the workspace.

mod ec;
#[cfg(test)]
mod ec_test;
mod err;
mod err_struct;
#[cfg(test)]
mod err_test;
pub mod others;
pub mod subsystem;

pub use ec::{ErrorCode, Severity};
pub use err::{ErrorSource, MuduError, ResultExt, StringError};

// Re-export convenience macros so they are available through `mudu::error::*`.
pub use crate::{bail, ensure, mudu_error};
