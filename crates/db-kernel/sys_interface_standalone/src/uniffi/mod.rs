mod async_;
mod sync;

/// Re-export the asynchronous UniFFI entry points.
pub use async_::*;
/// Re-export the synchronous UniFFI entry points.
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
