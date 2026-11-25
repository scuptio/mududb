//! The transport trait of the channel: a [`ChannelEndpoint`] delivers
//! envelopes between the registered endpoints of a server.

use async_trait::async_trait;
use mudu::channel::{EndpointId, Envelope, MessageId, MessageKind, OutgoingMessage, RecvFilter};
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

/// Boxed future run by a [`KindHandler`].
pub type ChannelCallbackFuture = Pin<Box<dyn Future<Output = RS<()>> + 'static>>;
/// Handler invoked when an envelope of the registered kind arrives.
pub type KindHandler = Arc<dyn Fn(Envelope) -> ChannelCallbackFuture + Send + Sync>;

/// Channel endpoint: the low-level transport a typed
/// [`Channel`](super::typed::Channel) layers on.
///
/// Implementations honor the semantics contract of
/// [`crate::channel`]: non-blocking at-most-once sends, deadline eviction,
/// bounded response buffering, and nack-on-unknown-kind.
#[async_trait]
pub trait ChannelEndpoint: Send + Sync {
    /// The endpoint id of this channel.
    fn local_endpoint(&self) -> EndpointId;

    /// Sends `message` to endpoint `dst` and returns the new message id.
    ///
    /// Low-level building block of the typed layer; prefer
    /// `Channel::endpoint::<M>(dst).ask(...)` / `.tell(...)`.
    #[doc(hidden)]
    async fn send(&self, dst: EndpointId, message: OutgoingMessage) -> RS<MessageId>;

    /// Receives the next message matching `filter`. `deadline_ms` (absolute,
    /// same clock as [`channel_now_ms`](super::registry::channel_now_ms))
    /// lets the implementation sweep the waiter once the caller's deadline
    /// has passed.
    ///
    /// Low-level building block of the typed layer; prefer
    /// `EndpointRef::ask`.
    #[doc(hidden)]
    async fn recv(&self, filter: RecvFilter, deadline_ms: Option<u64>) -> RS<Envelope>;

    /// Registers `handler` for envelopes of `kind` arriving with
    /// [`DeliveryMode::Request`](mudu::channel::DeliveryMode::Request) or
    /// [`DeliveryMode::FireAndForget`](mudu::channel::DeliveryMode::FireAndForget);
    /// fails with `ObjectAlreadyExists` when the kind is taken.
    ///
    /// Low-level building block of the typed layer; prefer
    /// `Channel::serve`.
    #[doc(hidden)]
    fn register_kind_handler(&self, kind: MessageKind, handler: KindHandler) -> RS<()>;

    /// Removes the handler of `kind`; a no-op when none is registered.
    #[doc(hidden)]
    fn unregister_kind_handler(&self, kind: MessageKind) -> RS<()>;

    /// Delivers an envelope addressed to this endpoint that arrived through
    /// another endpoint's transport (the registry fallback route for
    /// endpoints outside the transport's native routing table, e.g.
    /// non-worker threads).
    ///
    /// Low-level building block of the transports; not part of the typed
    /// API surface.
    #[doc(hidden)]
    fn deliver_incoming(&self, _envelope: Envelope) -> RS<()> {
        Err(mudu_error!(
            ErrorCode::NotImplemented,
            "this channel endpoint has no incoming delivery path"
        ))
    }

    /// Closes the endpoint: wakes every pending receiver with
    /// `ChannelClosed` and drops buffered messages and handlers. Called by
    /// [`EndpointGuard`](super::registry::EndpointGuard) on drop; the
    /// default is a no-op for transports with nothing to drain.
    fn close(&self) {}
}

/// Shared reference to a [`ChannelEndpoint`] implementation.
pub type ChannelEndpointRef = Arc<dyn ChannelEndpoint>;
