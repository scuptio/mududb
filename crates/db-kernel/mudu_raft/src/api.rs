//! Public result and status types of a raft node.
//!
//! Kept as a leaf module (dependencies are `core::conf` / `core::role` and
//! `mudu` only) so the driver in `node` can reference them without
//! depending back on the crate root.

use crate::core::conf::ConfVersion;
use crate::core::role::RaftRole;
use mudu::error::{ErrorCode, MuduError};
use mudu::mudu_error;

/// A snapshot of a node's observable raft state.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct NodeStatus {
    /// Node id.
    pub node_id: u64,
    /// Current role.
    pub role: RaftRole,
    /// Current term.
    pub term: u64,
    /// Currently known leader, if any.
    pub leader_id: Option<u64>,
    /// Current commit index.
    pub commit_index: u64,
    /// Version of the committed configuration. While this differs from
    /// `conf_version_new`, a re-configuration is in progress.
    pub conf_version_committed: ConfVersion,
    /// Version of the new (joint) configuration.
    pub conf_version_new: ConfVersion,
}

/// Error of a failed proposal.
///
/// Carries an optional leader hint for `NotLeader` failures so callers can
/// retry on the right node. Converts into [`MuduError`] (hint is embedded
/// in the message).
#[derive(Debug)]
pub struct ProposeError {
    error: MuduError,
    leader_hint: Option<u64>,
}

impl ProposeError {
    /// Creates an error from a plain `MuduError` without a leader hint.
    pub fn from_error(error: MuduError) -> Self {
        Self {
            error,
            leader_hint: None,
        }
    }

    /// Creates a `NotLeader` error carrying the given leader hint.
    pub fn not_leader(leader_hint: Option<u64>) -> Self {
        let message = match leader_hint {
            Some(id) => format!("not the raft leader (leader hint: {id})"),
            None => "not the raft leader (leader unknown)".to_string(),
        };
        Self {
            error: mudu_error!(ErrorCode::InvalidState, message),
            leader_hint,
        }
    }

    /// Creates the error returned when the node has already stopped.
    pub fn stopped() -> Self {
        Self {
            error: mudu_error!(
                ErrorCode::InvalidState,
                "raft node stopped before the proposal completed"
            ),
            leader_hint: None,
        }
    }

    /// The leader hint carried by `NotLeader` errors.
    pub fn leader_hint(&self) -> Option<u64> {
        self.leader_hint
    }

    /// The error code.
    pub fn ec(&self) -> ErrorCode {
        self.error.ec()
    }

    /// Converts into a plain [`MuduError`].
    pub fn into_mudu_error(self) -> MuduError {
        self.error
    }

    /// The underlying error reference.
    pub fn mudu_error(&self) -> &MuduError {
        &self.error
    }
}

impl std::fmt::Display for ProposeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.error)
    }
}

impl std::error::Error for ProposeError {}

impl From<ProposeError> for MuduError {
    fn from(e: ProposeError) -> Self {
        e.into_mudu_error()
    }
}

/// Result delivered to a `RaftProposeWaiter`: the `(index, term)` of
/// the committed entry, or a [`ProposeError`].
pub type ProposeResult = Result<(u64, u64), ProposeError>;

/// Error of a failed re-configuration request.
///
/// Immediate rejections (not the leader, a re-configuration already in
/// progress, invalid membership) are returned by
/// `RaftHandle::update_conf` itself; asynchronous failures
/// (leadership lost mid-reconfiguration, node stopped) are delivered through
/// `UpdateConfWaiter`. `NotLeader`-style errors carry an optional
/// leader hint.
#[derive(Debug)]
pub struct UpdateConfError {
    error: MuduError,
    leader_hint: Option<u64>,
}

impl UpdateConfError {
    /// Creates a `NotLeader` error carrying the given leader hint.
    pub fn not_leader(leader_hint: Option<u64>) -> Self {
        let message = match leader_hint {
            Some(id) => format!("not the raft leader (leader hint: {id})"),
            None => "not the raft leader (leader unknown)".to_string(),
        };
        Self {
            error: mudu_error!(ErrorCode::InvalidState, message),
            leader_hint,
        }
    }

    /// Creates the error returned when a re-configuration is already in progress.
    pub fn reconfig_in_progress() -> Self {
        Self {
            error: mudu_error!(
                ErrorCode::InvalidState,
                "a raft re-configuration is already in progress"
            ),
            leader_hint: None,
        }
    }

    /// Creates the error returned when leadership is lost mid-reconfiguration.
    pub fn leadership_lost(leader_hint: Option<u64>) -> Self {
        let message = match leader_hint {
            Some(id) => format!(
                "raft leadership was lost while the re-configuration was in progress (leader hint: {id})"
            ),
            None => {
                "raft leadership was lost while the re-configuration was in progress".to_string()
            }
        };
        Self {
            error: mudu_error!(ErrorCode::InvalidState, message),
            leader_hint,
        }
    }

    /// Creates the error returned when the node has already stopped.
    pub fn stopped() -> Self {
        Self {
            error: mudu_error!(
                ErrorCode::InvalidState,
                "raft node stopped before the re-configuration completed"
            ),
            leader_hint: None,
        }
    }

    /// Creates an error from a plain `MuduError` without a leader hint.
    pub fn from_error(error: MuduError) -> Self {
        Self {
            error,
            leader_hint: None,
        }
    }

    /// The leader hint carried by `NotLeader`-style errors.
    pub fn leader_hint(&self) -> Option<u64> {
        self.leader_hint
    }

    /// The error code.
    pub fn ec(&self) -> ErrorCode {
        self.error.ec()
    }

    /// Converts into a plain [`MuduError`].
    pub fn into_mudu_error(self) -> MuduError {
        self.error
    }

    /// The underlying error reference.
    pub fn mudu_error(&self) -> &MuduError {
        &self.error
    }
}

impl std::fmt::Display for UpdateConfError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.error)
    }
}

impl std::error::Error for UpdateConfError {}

impl From<UpdateConfError> for MuduError {
    fn from(e: UpdateConfError) -> Self {
        e.into_mudu_error()
    }
}

/// Result delivered to a `UpdateConfWaiter`: the committed
/// configuration version once the re-configuration completes.
pub type UpdateConfResult = Result<ConfVersion, UpdateConfError>;
