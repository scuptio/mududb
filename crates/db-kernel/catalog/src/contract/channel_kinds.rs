//! Central registry of the database kernel's channel message kinds.
//!
//! Every message type exchanged over a channel owns exactly one constant
//! here; requests and their responses share the kind and are told apart by
//! `DeliveryMode`. The namespace discipline (disjoint `0xYY00` blocks per
//! subsystem) and the allocation table live in [`mudu::channel`]; this file
//! registers the kernel's blocks: `0x71xx` partition RPC, `0x72xx`
//! management-plane RPC.

use mudu::channel::MessageKind;

/// Kind of the cross-partition request/response messages exchanged between
/// workers (`xcontract` partition RPC).
pub const PARTITION_RPC: MessageKind = MessageKind::User(0x7101);

/// Kind of the management-plane request/response messages served by workers
/// (queue activation control, queue observability; `xcontract`
/// `ManagementRequest`).
pub const MANAGEMENT_RPC: MessageKind = MessageKind::User(0x7201);

/// Kind of the control-plane client RPC (propose forwarding to the leader).
pub const CLUSTER_META_RPC: MessageKind = MessageKind::User(0x7302);

/// Kind of the control-plane forwarded-proposal tells (follower → control leader).
pub const CLUSTER_META_FORWARD: MessageKind = MessageKind::User(0x7303);

/// Kind of the epoch heartbeat tells (node → epoch leader).
pub const CLUSTER_EPOCH_TELL: MessageKind = MessageKind::User(0x7402);

/// Kind of the unified control-plane group's raft frames (node endpoint ↔
/// node endpoint); carries the merged meta/epoch control group.
pub const CLUSTER_CONTROL_RAFT: MessageKind = MessageKind::User(0x7501);

/// Kind of the control-plane external log stream push RPC: the leader ships
/// one durable-log frame to a follower, which acknowledges after fsync.
pub const CLUSTER_STREAM_PUSH: MessageKind = MessageKind::User(0x7502);

/// Kind of the control-plane external log stream fetch RPC: a RaftFollower pulls
/// one missing committed frame from a peer during heal/recovery.
pub const CLUSTER_STREAM_FETCH: MessageKind = MessageKind::User(0x7503);

/// Kind of the control-plane stream progress query RPC (the promotion
/// gate's catch-up check).
pub const CLUSTER_STREAM_PROGRESS: MessageKind = MessageKind::User(0x7504);

/// Kind of the control-plane state probe RPC (data-plane control client →
/// control node: leader/term/committed-frontier handshake).
pub const CLUSTER_CONTROL_STATE: MessageKind = MessageKind::User(0x7505);

/// Kind of the control-plane committed-records read RPC (data-plane control
/// client → control leader: committed catalog/epoch records from a LSN).
pub const CLUSTER_CONTROL_RECORDS: MessageKind = MessageKind::User(0x7506);
