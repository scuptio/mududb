//! Channel: point-to-point messaging between the endpoints of a server.
//!
//! A channel delivers typed envelopes from one endpoint to another. An
//! endpoint is any thread that registered a channel implementation under an
//! [`crate::channel::EndpointId`] — database workers are the primary inhabitants, but the
//! mechanism is a general inter-thread coordination facility: any thread
//! (background services, schedulers, drivers) may own an endpoint. The wire
//! model is deliberately small: every message carries a [`crate::channel::MessageKind`], a
//! delivery mode, an idempotency key (`request_id`), an absolute deadline,
//! and a reply status. Requests and responses share the same kind and are
//! told apart by [`crate::channel::DeliveryMode`], so one kind constant fully describes a
//! message type.
//!
//! This module is the pure wire protocol: types, the envelope, and the
//! typed-message traits. The runtime layer (transport trait, endpoint
//! registry, typed `Channel`/`ServiceRouter`) lives in
//! `mudu_utils::channel`; transport implementations live in the server
//! crate. Callers should prefer that typed layer over the raw
//! send/recv methods of the transport.
//!
//! # Delivery semantics
//!
//! Delivery is at-most-once: an implementation may drop a message (for
//! example past its deadline) but never duplicates one. Retries reuse the
//! same `request_id`, so receivers can deduplicate by `(src, request_id)`.
//! Sends never block; mailbox bounds and overflow behavior are documented
//! per implementation (the server implementations buffer waiter-less
//! responses up to a fixed cap, evicting expired entries and then the
//! oldest).
//!
//! # Kind namespace discipline
//!
//! [`crate::channel::MessageKind::System`] is reserved for channel-internal messages
//! (ack/nack). The `u16` tag of [`crate::channel::MessageKind::User`] is allocated to
//! subsystems in disjoint `0xYY00` blocks; every allocated block is listed
//! here and the individual kinds are registered in the owning subsystem's
//! kinds module. Current allocation:
//!
//! | Block    | Subsystem     | Registry module                  |
//! |----------|---------------|----------------------------------|
//! | `0x71xx` | partition RPC | `catalog::contract::channel_kinds` |
//! | `0x72xx` | management RPC | `catalog::contract::channel_kinds` |
//!
//! `0x7Fxx` is reserved for tests. Allocate a new block by adding a row
//! here in the same commit that introduces its first kind; never hand out
//! ad-hoc tags outside an allocated block.
//!
//! # Roadmap
//!
//! [`crate::channel::EndpointId`] is today a bare `OID`. When channels span processes the
//! routing table (not this type) gains the node qualification, and the
//! serde-derived envelope fields already survive a binary transport.

/// Envelope and outgoing-message types with their metadata accessors.
pub mod envelope;
/// Typed-message traits (`ChannelMessage`/`ChannelReply`) and `ServiceCtx`.
pub mod message;
/// Wire identity types: endpoint/message/request ids, kinds, delivery and
/// reply metadata, receive filters, and the `u128` serde adapters.
pub mod wire;

#[cfg(test)]
mod message_test;

pub use envelope::{Envelope, OutgoingMessage};
pub use message::{ChannelMessage, ChannelReply, ServiceCtx};
pub use wire::{
    DeliveryMode, EndpointId, MessageId, MessageKind, RecvFilter, ReplyStatus, RequestId,
    ServerInstanceId, SystemMessageKind,
};
