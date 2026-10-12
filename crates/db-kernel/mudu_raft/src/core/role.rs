//! Raft node role.
//!
//! Port of scupt-raft `raft_role.rs` (Apache-2.0, scuptio/scupt-raft).

use serde::{Deserialize, Serialize};

/// Role of a raft node.
#[derive(Copy, Clone, Hash, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub enum RaftRole {
    /// The node leads the cluster for the current term.
    Leader,
    /// The node follows a leader.
    #[default]
    Follower,
    /// The node is campaigning to become leader.
    Candidate,
    /// Reserved: a node that replicates the log but never votes.
    Learner,
}
