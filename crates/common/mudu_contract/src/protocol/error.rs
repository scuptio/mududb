//! Error response types carried by protocol frames.

use mudu::error::ErrorCode;
use mudu::error::MuduError;
use serde::{Deserialize, Serialize};

/// Optional hint pointing at the current replication leader, attached to
/// error responses so a replica or client can reconnect to the right node.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LeaderHint {
    host: String,
    port: u16,
}

/// Error response carrying a Mudu error code and message.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ErrorResponse {
    #[serde(default)]
    code: u32,
    #[serde(default)]
    name: String,
    message: String,
    #[serde(default)]
    source: String,
    #[serde(default)]
    location: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    leader_hint: Option<LeaderHint>,
}

impl ErrorResponse {
    /// Creates an internal error response from a message.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            code: ErrorCode::Internal.to_u32(),
            name: "Internal".to_string(),
            message: message.into(),
            source: String::new(),
            location: String::new(),
            leader_hint: None,
        }
    }

    /// Returns the error message.
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Returns the error code.
    pub fn code(&self) -> u32 {
        self.code
    }

    /// Returns the error code name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the error source chain as a JSON string.
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Returns the source location where the error was raised.
    pub fn location(&self) -> &str {
        &self.location
    }

    /// Builds an error response from a Mudu error.
    pub fn from_merror(error: &MuduError) -> Self {
        Self {
            code: error.ec().to_u32(),
            name: format!("{:?}", error.ec()),
            message: error.message().to_string(),
            source: error.err_src().to_json_str(),
            location: error.loc().to_string(),
            leader_hint: None,
        }
    }

    /// Attaches a replication leader hint to the error response.
    pub fn with_leader_hint(mut self, leader_hint: LeaderHint) -> Self {
        self.leader_hint = Some(leader_hint);
        self
    }

    /// Returns the replication leader hint, if attached.
    pub fn leader_hint(&self) -> Option<&LeaderHint> {
        self.leader_hint.as_ref()
    }
}

impl LeaderHint {
    /// Creates a leader hint for the given host and port.
    pub fn new(host: impl Into<String>, port: u16) -> Self {
        Self {
            host: host.into(),
            port,
        }
    }

    /// Host name or address of the current leader.
    pub fn host(&self) -> &str {
        &self.host
    }

    /// Port the current leader listens on.
    pub fn port(&self) -> u16 {
        self.port
    }
}
