use async_trait::async_trait;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_sys::sync::SMutex;
use std::cell::UnsafeCell;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, OnceLock};

/// Unique id of a message, handed out by [`MessageBus::send`].
pub type MessageId = u64;
/// Id of a registered receive-callback subscription, used to cancel it.
pub type SubscriptionId = u64;
/// Boxed future returned by an [`OnRecvCallback`].
pub type MessageCallbackFuture = Pin<Box<dyn Future<Output = RS<()>> + 'static>>;
/// Callback invoked when a message matching the subscription filter arrives.
pub type OnRecvCallback = Arc<dyn Fn(Envelope) -> MessageCallbackFuture + 'static>;

thread_local! {
    static CURRENT_MESSAGE_BUS: UnsafeCell<Option<MessageBusRef>> =
        const { UnsafeCell::new(None) };
}

/// Runtime message-bus endpoint id.
///
/// Today all message-bus endpoints are worker-local endpoints, so this id is
/// the worker id allocated and maintained by `WorkerRegistry`. `send` uses it
/// to route to the target worker's mailbox, and `recv` uses it to match message
/// source/destination filters.
pub type EndpointId = OID;

/// Delivery semantics requested for an outgoing message.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DeliveryMode {
    /// No reply is expected.
    FireAndForget,
    /// A reply correlated by message id is expected.
    Request,
    /// The message replies to an earlier request.
    Response,
}

/// Kind of a bus-internal (system) message.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SystemMessageKind {
    /// Positive acknowledgement.
    Ack,
    /// Negative acknowledgement.
    Nack,
}

/// Kind of a message: application-defined or bus-internal.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MessageKind {
    /// Application-defined kind, identified by a user tag.
    User(u16),
    /// Bus-internal message.
    System(SystemMessageKind),
}

/// A delivered message with its routing, correlation, and delivery metadata.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Envelope {
    msg_id: MessageId,
    correlation_id: Option<MessageId>,
    src: EndpointId,
    dst: EndpointId,
    kind: MessageKind,
    payload: Vec<u8>,
    delivery: DeliveryMode,
}

/// Filter selecting the messages a receive or subscription matches; unset
/// fields match everything.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct RecvFilter {
    /// Match only messages from this endpoint.
    pub src: Option<EndpointId>,
    /// Match only messages addressed to this endpoint.
    pub dst: Option<EndpointId>,
    /// Match only messages of this kind.
    pub kind: Option<MessageKind>,
    /// Match only messages correlated to this message id.
    pub correlation_id: Option<MessageId>,
}

/// A message to send, with optional correlation id and delivery mode.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutgoingMessage {
    kind: MessageKind,
    payload: Vec<u8>,
    correlation_id: Option<MessageId>,
    delivery: DeliveryMode,
}

/// Worker-local message bus endpoint.
#[async_trait]
pub trait MessageBus: Send + Sync {
    /// The endpoint id of this bus.
    fn local_endpoint(&self) -> EndpointId;

    /// Sends `message` to endpoint `dst` and returns the new message id.
    async fn send(&self, dst: EndpointId, message: OutgoingMessage) -> RS<MessageId>;

    /// Receives the next message matching `filter`.
    async fn recv(&self, filter: RecvFilter) -> RS<Envelope>;

    /// Registers `callback` for messages matching `filter` and returns the
    /// subscription id.
    fn on_recv_callback(&self, filter: RecvFilter, callback: OnRecvCallback) -> RS<SubscriptionId>;

    /// Cancels the subscription `id`; returns whether one was registered.
    fn cancel_callback(&self, id: SubscriptionId) -> RS<bool>;
}

/// Shared reference to a [`MessageBus`] implementation.
pub type MessageBusRef = Arc<dyn MessageBus>;
/// Id of a server instance; scopes the worker message-bus registry.
pub type ServerInstanceId = OID;

fn message_bus_registry() -> &'static SMutex<HashMap<(ServerInstanceId, OID), MessageBusRef>> {
    static REGISTRY: OnceLock<SMutex<HashMap<(ServerInstanceId, OID), MessageBusRef>>> =
        OnceLock::new();
    REGISTRY.get_or_init(|| SMutex::new(HashMap::new()))
}

impl Envelope {
    /// Creates an envelope with explicit routing and correlation metadata.
    pub fn new(
        msg_id: MessageId,
        correlation_id: Option<MessageId>,
        src: EndpointId,
        dst: EndpointId,
        kind: MessageKind,
        payload: Vec<u8>,
        delivery: DeliveryMode,
    ) -> Self {
        Self {
            msg_id,
            correlation_id,
            src,
            dst,
            kind,
            payload,
            delivery,
        }
    }

    /// The message id.
    pub fn msg_id(&self) -> MessageId {
        self.msg_id
    }

    /// The id of the request this message replies to, if any.
    pub fn correlation_id(&self) -> Option<MessageId> {
        self.correlation_id
    }

    /// The sender endpoint.
    pub fn src(&self) -> &EndpointId {
        &self.src
    }

    /// The destination endpoint.
    pub fn dst(&self) -> &EndpointId {
        &self.dst
    }

    /// The message kind.
    pub fn kind(&self) -> MessageKind {
        self.kind
    }

    /// The payload bytes.
    pub fn payload(&self) -> &[u8] {
        &self.payload
    }

    /// A copy of the payload bytes.
    pub fn payload_owned(&self) -> Vec<u8> {
        self.payload.clone()
    }

    /// The delivery mode.
    pub fn delivery(&self) -> DeliveryMode {
        self.delivery
    }

    /// Whether this envelope satisfies every set field of `filter`.
    pub fn matches(&self, filter: &RecvFilter) -> bool {
        filter.src.as_ref().is_none_or(|src| src == self.src())
            && filter.dst.as_ref().is_none_or(|dst| dst == self.dst())
            && filter.kind.is_none_or(|kind| kind == self.kind())
            && filter
                .correlation_id
                .is_none_or(|correlation_id| Some(correlation_id) == self.correlation_id())
    }
}

impl OutgoingMessage {
    /// Creates a fire-and-forget message of the given kind.
    pub fn new(kind: MessageKind, payload: Vec<u8>) -> Self {
        Self {
            kind,
            payload,
            correlation_id: None,
            delivery: DeliveryMode::FireAndForget,
        }
    }

    /// Sets the id of the request this message replies to.
    pub fn with_correlation_id(mut self, correlation_id: MessageId) -> Self {
        self.correlation_id = Some(correlation_id);
        self
    }

    /// Sets the delivery mode.
    pub fn with_delivery(mut self, delivery: DeliveryMode) -> Self {
        self.delivery = delivery;
        self
    }

    /// The message kind.
    pub fn kind(&self) -> MessageKind {
        self.kind
    }

    /// The payload bytes.
    pub fn payload(&self) -> &[u8] {
        &self.payload
    }

    /// A copy of the payload bytes.
    pub fn payload_owned(&self) -> Vec<u8> {
        self.payload.clone()
    }

    /// The id of the request this message replies to, if any.
    pub fn correlation_id(&self) -> Option<MessageId> {
        self.correlation_id
    }

    /// The delivery mode.
    pub fn delivery(&self) -> DeliveryMode {
        self.delivery
    }
}

/// Installs `message_bus` as the current thread's message bus.
pub fn set_current_message_bus(message_bus: MessageBusRef) {
    CURRENT_MESSAGE_BUS.with(|slot| {
        // Safety: the slot is thread-local and only mutated through these helpers.
        unsafe {
            *slot.get() = Some(message_bus);
        }
    });
}

/// Removes the current thread's message bus.
pub fn unset_current_message_bus() {
    CURRENT_MESSAGE_BUS.with(|slot| {
        // Safety: the slot is thread-local and only mutated through these helpers.
        unsafe {
            *slot.get() = None;
        }
    });
}

/// Returns the current thread's message bus; errors when none is installed.
pub fn current_message_bus() -> RS<MessageBusRef> {
    CURRENT_MESSAGE_BUS.with(|slot| {
        // Safety: shared reads are confined to the current thread-local slot.
        let message_bus = unsafe { &*slot.get() };
        message_bus
            .as_ref()
            .cloned()
            .ok_or_else(|| mudu_error!(ErrorCode::EntityNotFound, "current message bus is not set"))
    })
}

/// Registers the message bus of worker `worker_id` under the given server
/// instance.
pub fn register_worker_message_bus(
    server_instance_id: ServerInstanceId,
    worker_id: OID,
    message_bus: &MessageBusRef,
) -> RS<()> {
    let mut registry = message_bus_registry()
        .lock()
        .map_err(|_| mudu_error!(ErrorCode::Internal, "message bus registry lock poisoned"))?;
    registry.insert((server_instance_id, worker_id), message_bus.clone());
    Ok(())
}

/// Removes the message-bus registration of worker `worker_id`; a no-op when
/// none is registered.
pub fn unregister_worker_message_bus(
    server_instance_id: ServerInstanceId,
    worker_id: OID,
) -> RS<()> {
    let mut registry = message_bus_registry()
        .lock()
        .map_err(|_| mudu_error!(ErrorCode::Internal, "message bus registry lock poisoned"))?;
    let Some(_bus) = registry.remove(&(server_instance_id, worker_id)) else {
        return Ok(());
    };
    Ok(())
}

/// Looks up the message bus registered for worker `worker_id` of the given
/// server instance; errors when none is registered.
pub fn message_bus_for_worker(
    server_instance_id: ServerInstanceId,
    worker_id: OID,
) -> RS<MessageBusRef> {
    let registry = message_bus_registry()
        .lock()
        .map_err(|_| mudu_error!(ErrorCode::Internal, "message bus registry lock poisoned"))?;
    registry
        .get(&(server_instance_id, worker_id))
        .cloned()
        .ok_or_else(|| {
            mudu_error!(
                ErrorCode::EntityNotFound,
                format!(
                    "message bus for server {} worker {} is not registered",
                    server_instance_id, worker_id
                )
            )
        })
}
