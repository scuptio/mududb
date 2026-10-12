//! The result type used throughout the workspace.

use crate::error::MuduError;

/// Result with [`MuduError`] as the error type.
pub type RS<X> = Result<X, MuduError>;
