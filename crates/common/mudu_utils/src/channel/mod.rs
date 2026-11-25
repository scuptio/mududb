//! Channel runtime layer: transport trait, endpoint registry, and the typed
//! ask/tell API over the pure wire protocol of [`mudu::channel`].
//!
//! An endpoint is any thread that registered a [`crate::channel::ChannelEndpoint`]
//! implementation under an allocated [`EndpointId`](mudu::channel::EndpointId)
//! via [`crate::channel::spawn_endpoint`] (or the lower-level [`crate::channel::register_channel`]); database
//! workers are the primary inhabitants, but the mechanism is a general
//! inter-thread coordination facility. [`crate::channel::EndpointGuard`] owns the
//! registration: dropping it unregisters the endpoint and closes its
//! transport.
//!
//! # Semantics contract
//!
//! Transport implementations honor these rules; the typed layer and callers
//! rely on them:
//!
//! - `send` never blocks; delivery is at-most-once. A message past its
//!   deadline fails the send or is dropped on arrival.
//! - Responses with no matching waiter are buffered for a later `recv`;
//!   the buffer is bounded per implementation (the server implementations
//!   cap it at 1024, evicting expired entries first, then the oldest), so a
//!   slow asker can lose a reply and observe a timeout instead.
//! - A request arriving for a kind with no registered handler is nacked
//!   back to the asker; a fire-and-forget message of an unknown kind is
//!   dropped.
//! - [`crate::channel::ChannelEndpoint::close`] wakes every pending receiver with
//!   `ChannelClosed` and drops buffered state and handlers.
//!
//! The serving model is concurrent: every incoming message runs its handler
//! as a fresh task (per-process serial mailbox semantics are deliberately
//! not adopted — state shared between handlers lives behind the storage /
//! transaction layers' own synchronization).

/// Transport trait (`ChannelEndpoint`) and handler types.
pub mod endpoint;
/// Endpoint registry, `spawn_endpoint`/`EndpointGuard` lifecycle, the
/// current-thread channel slot, and the channel clock/id helpers.
pub mod registry;
/// Typed layer: `Channel`, `EndpointRef`, `ServiceRouter`, `ServiceHandle`.
pub mod typed;

pub use endpoint::{ChannelCallbackFuture, ChannelEndpoint, ChannelEndpointRef, KindHandler};
pub use registry::{
    EndpointGuard, channel_for, current_channel, register_channel, set_current_channel,
    spawn_endpoint, spawn_endpoint_for_current_thread, unregister_channel, unset_current_channel,
};
pub use typed::{
    Channel, EndpointRef, ServiceHandle, ServiceRouter, channel_now_ms, gen_request_id,
};
