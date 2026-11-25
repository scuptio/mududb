//! The unified control plane's pure contracts: one Raft replication
//! group commits only stream progress references while catalog and epoch
//! control records replicate through external log streams.
//!
//! This module carries only constants and wire types; the mechanism lives in
//! `repl::log_stream` / `repl::control_plane`, and the network assembly in
//! `server`. The wire types mirror the replication mechanism's round/ack
//! identities without depending on `mudu_raft` (layering: `catalog` is pure
//! contracts).

use serde::{Deserialize, Serialize};

/// Stable replication-group id of the unified control plane.
pub const CONTROL_GROUP_ID: u64 = 1;

/// Stream id of the catalog (metadata) control stream.
pub const CONTROL_STREAM_META: u64 = 1;
/// Stream id of the epoch control stream.
pub const CONTROL_STREAM_EPOCH: u64 = 2;

/// Adapter kind of the catalog control stream.
pub const CONTROL_STREAM_META_KIND: &str = "control.meta";
/// Adapter kind of the epoch control stream.
pub const CONTROL_STREAM_EPOCH_KIND: &str = "control.epoch";

/// Payload codec version of the control streams (rmp-serde of the op enums).
pub const CONTROL_STREAM_CODEC_VERSION: u32 = 1;
/// Digest version of the control streams (CRC64-derived chain; a corruption
/// check, not a cryptographic hash).
pub const CONTROL_STREAM_DIGEST_VERSION: u32 = 1;

/// The exact durability round of one external stream segment: the wire mirror
/// of the replication mechanism's `AckRound`. A higher LSN is never accepted
/// as proof of a different digest.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StreamRoundWire {
    /// Cluster namespace.
    pub cluster_id: u64,
    /// Replication group.
    pub group_id: u64,
    /// Recovery generation fencing previous group instances.
    pub group_generation: u64,
    /// Stream registry interpretation used for this round.
    pub registry_version: u64,
    /// Leader term that admitted the segment.
    pub leader_term: u64,
    /// Committed voter configuration as `(term, version, index)`.
    pub committed_conf: (u64, u64, u64),
    /// Incoming voter configuration as `(term, version, index)`; equals the
    /// committed triple outside joint consensus.
    pub incoming_conf: (u64, u64, u64),
    /// Adapter payload codec version.
    pub codec_version: u32,
    /// Digest algorithm version.
    pub digest_version: u32,
    /// Stream identity.
    pub stream_id: u64,
    /// Stream incarnation.
    pub stream_generation: u64,
    /// Inclusive last LSN of the segment.
    pub end_lsn: u64,
    /// Versioned segment digest.
    pub segment_digest: [u8; 32],
}

/// Leader → member push of one external-log frame; the member acknowledges
/// only after it durably holds the exact entire prefix named by `round`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StreamPushRequest {
    /// Session token the leader assigned to this connection epoch.
    pub session_id: u128,
    /// The sender's node id (the receiver cross-checks it against the
    /// current group leader before any durable write).
    pub from_node: u64,
    /// The exact durability round.
    pub round: StreamRoundWire,
    /// One LGFM frame of the stream's durable log.
    pub frame: Vec<u8>,
}

/// Member's durable receipt; `None` while the member holds only a proper
/// prefix of the round (intermediate catch-up frames).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StreamPushResponse {
    /// The durable ack, when the member holds the exact round prefix.
    pub ack: Option<StreamAckWire>,
}

/// A member's durable receipt of one exact round.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StreamAckWire {
    /// The acknowledging member's node id.
    pub node_id: u64,
    /// Session token echoed from the request.
    pub session_id: u128,
    /// The exact round durably held.
    pub round: StreamRoundWire,
}

/// Member → member fetch of one durable frame during heal/recovery.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StreamFetchRequest {
    /// Session token of the requester's connection epoch.
    pub session_id: u128,
    /// The requester's node id (the serving side admits only group members).
    pub from_node: u64,
    /// Cluster namespace.
    pub cluster_id: u64,
    /// Replication group.
    pub group_id: u64,
    /// Recovery generation.
    pub group_generation: u64,
    /// Stream identity.
    pub stream_id: u64,
    /// Stream incarnation.
    pub stream_generation: u64,
    /// Exactly one requested LSN.
    pub lsn: u64,
}

/// The fetched durable frame.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StreamFetchResponse {
    /// One LGFM frame of the stream's durable log.
    pub frame: Vec<u8>,
}

/// Member → member query of a durable stream progress at an exact LSN (the
/// promotion gate's catch-up check).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StreamProgressRequest {
    /// Session token of the requester's connection epoch.
    pub session_id: u128,
    /// The requester's node id.
    pub from_node: u64,
    /// Cluster namespace.
    pub cluster_id: u64,
    /// Replication group.
    pub group_id: u64,
    /// Recovery generation.
    pub group_generation: u64,
    /// Stream identity.
    pub stream_id: u64,
    /// Exactly one queried LSN.
    pub lsn: u64,
}

/// The member's durable progress at the queried LSN, when held.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StreamProgressResponse {
    /// The exact progress at the LSN.
    pub progress: Option<StreamProgressWire>,
}

/// Wire form of one stream's exact durable prefix reference.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StreamProgressWire {
    /// Stream identity.
    pub stream_id: u64,
    /// Stream incarnation.
    pub stream_generation: u64,
    /// Inclusive last LSN.
    pub end_lsn: u64,
    /// Versioned segment digest.
    pub segment_digest: [u8; 32],
}

mudu::impl_channel_message!(
    StreamPushRequest,
    reply = StreamPushResponse,
    kind = crate::contract::channel_kinds::CLUSTER_STREAM_PUSH
);

mudu::impl_channel_message!(
    StreamFetchRequest,
    reply = StreamFetchResponse,
    kind = crate::contract::channel_kinds::CLUSTER_STREAM_FETCH
);

mudu::impl_channel_message!(
    StreamProgressRequest,
    reply = StreamProgressResponse,
    kind = crate::contract::channel_kinds::CLUSTER_STREAM_PROGRESS
);

/// Data-plane control client → control node: status probe and committed
/// frontier handshake. A client sends this to discover the current control
/// leader, its term, and the committed frontiers of the control streams,
/// without joining the control Raft group.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ControlPlaneStateRequest {
    /// Cluster namespace.
    pub cluster_id: u64,
    /// Control replication group.
    pub group_id: u64,
    /// Recovery generation.
    pub group_generation: u64,
}

/// The control node's answer to a state probe.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ControlPlaneStateResponse {
    /// Whether the answering node is the current control leader.
    pub is_leader: bool,
    /// The current control leader, when known.
    pub leader_id: Option<u64>,
    /// The current control consensus term.
    pub term: u64,
    /// The committed frontier of every live control stream.
    pub committed: Vec<StreamProgressWire>,
    /// The leader's current epoch watermarks (stable/current may run ahead of
    /// the committed epoch records under the sparse epoch protocol, bounded by
    /// the committed `reserved_upper`).
    pub epoch: EpochStateWire,
}

/// Wire mirror of the epoch oracle watermarks a control client needs: the
/// stable/current edges and the committed reserved upper bound.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EpochStateWire {
    /// Cluster stable epoch.
    pub stable: u64,
    /// Epoch collecting commits.
    pub current: u64,
    /// Committed inclusive upper bound of reserved epoch numbers.
    pub reserved_upper: u64,
}

/// Data-plane control client → control leader: read the committed records of
/// one control stream starting at `from_lsn`, with the client's last applied
/// prefix for continuity. The leader answers only from its committed prefix,
/// never speculative data.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ControlStructsRequest {
    /// Cluster namespace.
    pub cluster_id: u64,
    /// Control replication group.
    pub group_id: u64,
    /// Recovery generation.
    pub group_generation: u64,
    /// Stream identity.
    pub stream_id: u64,
    /// Stream incarnation.
    pub stream_generation: u64,
    /// First requested LSN (the client's applied end LSN + 1).
    pub from_lsn: u64,
    /// The client's last applied prefix, when any, for continuity.
    pub applied: Option<StreamProgressWire>,
}

/// The leader's answer: the authoritative committed prefix plus every
/// committed record from `from_lsn` through it, or a resync signal.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ControlStructsResponse {
    /// When true, the client's namespace or applied prefix does not line up
    /// with the committed history and it must reset and re-sync from scratch.
    pub resync: bool,
    /// The committed frontier of the requested stream (`None` when resyncing
    /// because the namespaces disagree).
    pub committed: Option<StreamProgressWire>,
    /// Committed records in ascending LSN order, starting at `from_lsn`.
    pub records: Vec<ControlStructWire>,
}

/// One committed control record (decoded payload; the leader's durable log
/// already authenticated the LSN). `progress` is the record's exact durable
/// prefix (with digest), so the client persists a verifiable applied frontier;
/// `request_id` lets the client resolve a propose's local-apply wait.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ControlStructWire {
    /// The record's LSN.
    pub lsn: u64,
    /// The record's idempotency key.
    pub request_id: u128,
    /// The record's exact durable prefix.
    pub progress: StreamProgressWire,
    /// The encoded control operation (`MetaOp` or `EpochOp`).
    pub payload: Vec<u8>,
}

mudu::impl_channel_message!(
    ControlPlaneStateRequest,
    reply = ControlPlaneStateResponse,
    kind = crate::contract::channel_kinds::CLUSTER_CONTROL_STATE
);

mudu::impl_channel_message!(
    ControlStructsRequest,
    reply = ControlStructsResponse,
    kind = crate::contract::channel_kinds::CLUSTER_CONTROL_RECORDS
);
