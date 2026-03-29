mod async_;
mod sync;

pub use async_::*;
pub use sync::*;

use thiserror::Error;

#[derive(Debug, Error, ::uniffi::Error)]
pub enum SysInterfaceUniffiError {
    #[error("{0}")]
    Message(String),
}

pub(crate) fn binding_error(err: impl ToString) -> SysInterfaceUniffiError {
    SysInterfaceUniffiError::Message(err.to_string())
}
