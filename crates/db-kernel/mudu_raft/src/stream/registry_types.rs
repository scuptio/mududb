//! Versioned protocol records bootstrapping the business stream registry.

use super::{GroupId, StreamId};
use serde::{Deserialize, Serialize};

/// Version of the registry management protocol.
pub const REGISTRY_FORMAT_VERSION: u32 = 1;

/// Namespace and recovery authority of one replication group.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupIdobject {
    /// Cluster namespace.
    pub cluster_id: u64,
    /// Stable group identifier.
    pub group_id: GroupId,
    /// Recovery generation fencing previous instances.
    pub generation: u64,
}

/// Resource limits allocated to a stream, independent of worker identities.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StreamBudget {
    /// Largest individual record in bytes.
    pub max_record_bytes: u32,
    /// Maximum outstanding payload bytes.
    pub max_inflight_bytes: u64,
    /// Scheduling priority; larger values receive higher priority.
    pub priority: u8,
}

/// Persisted stream identity, format and resource allocation. Its enclosing
/// registry record supplies the cluster and replication-group namespace.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StreamDescriptor {
    /// Stable stream identifier, never reused after removal.
    pub stream_id: StreamId,
    /// Stream incarnation, distinct from the group recovery generation.
    pub stream_generation: u64,
    /// Business adapter kind; the replication mechanism does not interpret it.
    pub kind: String,
    /// Adapter payload codec version.
    pub codec_version: u32,
    /// Segment digest algorithm version. CRC-based digests are corruption
    /// checks, not cryptographic commitments.
    pub digest_version: u32,
    /// Admission and scheduling allocation.
    pub budget: StreamBudget,
    /// Persistent stream dependencies protecting their registry identities
    /// from removal. Per-record LSN dependencies belong to the adapter records.
    pub requires: Vec<StreamId>,
}

/// One operation on the durable stream registry.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RegistryOp {
    /// Register an adapter's immutable descriptor.
    Register(StreamDescriptor),
    /// Reject new writes and begin draining the stream.
    Deactivate {
        /// Stream to drain.
        stream_id: StreamId,
        /// Expected incarnation.
        stream_generation: u64,
    },
    /// Preserve a permanent identity tombstone. Nonempty streams require
    /// a later data/GC retirement protocol and cannot yet be removed.
    Remove {
        /// Empty, drained stream to remove.
        stream_id: StreamId,
        /// Expected incarnation.
        stream_generation: u64,
    },
}

/// A registry mutation carried directly in a Raft management entry, so stream
/// registration never depends on the existence of a business log stream.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegistryChange {
    /// Management record format version.
    pub format_version: u32,
    /// Group whose registry is changing.
    pub identity: GroupIdobject,
    /// Stable operation identity; retries preserve the committed result.
    pub request_id: u128,
    /// Registry version against which the mutation was planned.
    pub previous_version: u64,
    /// Mutation to apply after Raft commit.
    pub operation: RegistryOp,
}
