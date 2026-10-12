use std::error::Error;
use std::fmt::{Display, Formatter};

/// Error produced while configuring a backend or encoding/decoding MSSP
/// frames.
///
/// Business errors reported by the host are *not* `ApiError`s: they travel
/// inside result frames as `UniError` values.
#[derive(Debug)]
pub enum ApiError {
    /// No backend is configured (or the selected adapter is unusable).
    BackendUnavailable(&'static str),
    /// A request could not be encoded into an MSSP frame.
    Encode(String),
    /// A response frame was malformed or carried an error status.
    Decode(String),
    /// A blocking mock task failed to join.
    Join(String),
}

impl ApiError {
    /// Builds an [`ApiError::BackendUnavailable`] with a static message.
    pub fn backend_unavailable(message: &'static str) -> Self {
        Self::BackendUnavailable(message)
    }
}

impl Display for ApiError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BackendUnavailable(message) => write!(f, "{message}"),
            Self::Encode(message) => write!(f, "encode error: {message}"),
            Self::Decode(message) => write!(f, "decode error: {message}"),
            Self::Join(message) => write!(f, "task join error: {message}"),
        }
    }
}

impl Error for ApiError {}

impl From<rmp_serde::encode::Error> for ApiError {
    fn from(value: rmp_serde::encode::Error) -> Self {
        Self::Encode(value.to_string())
    }
}

impl From<rmp_serde::decode::Error> for ApiError {
    fn from(value: rmp_serde::decode::Error) -> Self {
        Self::Decode(value.to_string())
    }
}

impl From<tokio::task::JoinError> for ApiError {
    fn from(value: tokio::task::JoinError) -> Self {
        Self::Join(value.to_string())
    }
}
