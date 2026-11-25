//! Business-independent identities and durable commit references for log streams.

mod registry_types;
/// Versioned stream registration and group namespace types.
pub use registry_types::{
    GroupIdobject, RegistryChange, RegistryOp, StreamBudget, StreamDescriptor,
    REGISTRY_FORMAT_VERSION,
};

use crate::core::conf::{ConfNodeValue, ConfVersion};
use serde::{Deserialize, Serialize};

/// Stable replication group identity, independent of nodes and workers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct GroupId(pub u64);

/// Stable stream identity within a replication group.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct StreamId(pub u64);

/// The exact old/new configuration pair used for a data durability decision.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MembershipVersion {
    /// Committed voter configuration.
    pub committed: ConfVersion,
    /// Incoming voter configuration, equal to committed outside joint consensus.
    pub incoming: ConfVersion,
}

impl MembershipVersion {
    /// Captures the configuration pair from the serialized protocol owner.
    pub fn from_conf(committed: &ConfNodeValue, incoming: &ConfNodeValue) -> Self {
        Self {
            committed: committed.term_version().clone(),
            incoming: incoming.term_version().clone(),
        }
    }

    /// Checks that a durability proof still applies immediately before proposal.
    pub fn matches(&self, committed: &ConfNodeValue, incoming: &ConfNodeValue) -> bool {
        self.committed == *committed.term_version() && self.incoming == *incoming.term_version()
    }
}

/// One stream's committed durable prefix. LSNs are comparable only within
/// the same group, stream and stream generation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StreamProgress {
    /// Stream in the durable registry.
    pub stream_id: StreamId,
    /// Generation prevents reuse of an old stream's acknowledgements.
    pub stream_generation: u64,
    /// Inclusive last committed LSN.
    pub end_lsn: u64,
    /// Versioned digest bytes; the descriptor identifies the algorithm.
    pub segment_digest: [u8; 32],
}

/// A delta to a group's authoritative progress vector. Business payloads
/// are replicated through log streams before this reference is proposed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupCommit {
    /// Cluster namespace of this group.
    pub cluster_id: u64,
    /// Replication group, independent of stream kind.
    pub group_id: GroupId,
    /// Recovery generation fencing previous group instances.
    pub group_generation: u64,
    /// Configuration pair whose quorums durably hold the referenced payloads.
    pub membership_version: MembershipVersion,
    /// Durable stream registry version needed to interpret this vector.
    pub registry_version: u64,
    /// Stream advances; omitted streams keep their previous frontier.
    pub progress: Vec<StreamProgress>,
}
