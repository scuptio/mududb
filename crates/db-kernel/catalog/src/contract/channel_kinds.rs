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
