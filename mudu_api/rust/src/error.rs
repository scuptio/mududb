use std::error::Error;
use std::fmt::{Display, Formatter};

#[derive(Debug)]
pub enum ApiError {
    BackendUnavailable(&'static str),
    Encode(String),
    Decode(String),
    Join(String),
}

impl ApiError {
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
