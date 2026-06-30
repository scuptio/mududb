mod async_;
mod sync;

/// Re-export the platform-specific implementation.
pub use async_::*;
/// Re-export the platform-specific implementation.
pub use sync::*;

use thiserror::Error;

#[derive(Debug, Error, ::uniffi::Error)]
/// Error type returned across UniFFI bindings.
pub enum SysInterfaceUniffiError {
    /// A textual error message.
    #[error("{0}")]
    Message(String),
}

pub(crate) fn binding_error(err: impl ToString) -> SysInterfaceUniffiError {
    SysInterfaceUniffiError::Message(err.to_string())
}
