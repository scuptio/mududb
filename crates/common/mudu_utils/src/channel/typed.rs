//! Typed request/response layer over the raw channel transport.
//!
//! This layer gives the channel its ergonomic surface: callers describe a
//! message type once ([`ChannelMessage`](mudu::channel::ChannelMessage),
//! usually via [`impl_channel_message!`](mudu::impl_channel_message)) and
//! then either ask an endpoint and await a typed reply
//! ([`EndpointRef::ask`]) or serve requests with a typed handler
//! ([`ServiceRouter`]). Encoding, correlation, deadlines, timeouts, and
//! nack propagation are handled here; services never touch envelopes.

use super::endpoint::{ChannelCallbackFuture, ChannelEndpointRef, KindHandler};
use mudu::channel::{
    ChannelMessage, ChannelReply, DeliveryMode, EndpointId, Envelope, MessageKind, OutgoingMessage,
    RecvFilter, ReplyStatus, RequestId, ServiceCtx,
};
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use std::collections::HashMap;
use std::future::Future;
use std::marker::PhantomData;
use std::sync::Arc;
use std::time::Duration;

/// Milliseconds since the Unix epoch on the channel clock: the time base of
/// `deadline_ms` fields and waiter sweeping.
pub fn channel_now_ms() -> u64 {
    mudu_sys::time::system_time_now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64
}

/// Generates a fresh idempotency key for a logical channel operation.
pub fn gen_request_id() -> mudu::channel::RequestId {
    mudu_sys::random::uuid_v4().as_u128()
}

/// Typed view of a [`ChannelEndpointRef`]: endpoint handles and service
/// registration.
#[derive(Clone)]
pub struct Channel {
    inner: ChannelEndpointRef,
}

/// A typed handle to one endpoint for one message type.
///
/// Cheap to clone; carries no per-call state.
pub struct EndpointRef<M: ChannelMessage> {
    inner: ChannelEndpointRef,
    dst: EndpointId,
    _marker: PhantomData<fn() -> M>,
}

/// A typed service: a set of message handlers keyed by kind, registered on
/// a channel in one shot by [`Channel::serve`].
pub struct ServiceRouter {
    handlers: HashMap<MessageKind, RouterEntry>,
}

type RouterEntry =
    Arc<dyn Fn(&ChannelEndpointRef, Envelope) -> ChannelCallbackFuture + Send + Sync>;

/// Keeps a service's kind handlers registered; dropping the handle
/// unregisters them (RAII).
pub struct ServiceHandle {
    inner: ChannelEndpointRef,
    kinds: Vec<MessageKind>,
}

impl Channel {
    /// Wraps a raw channel reference.
    pub fn new(inner: ChannelEndpointRef) -> Self {
        Self { inner }
    }

    /// The underlying raw channel.
    pub fn raw(&self) -> &ChannelEndpointRef {
        &self.inner
    }

    /// The endpoint id of this channel.
    pub fn local_endpoint(&self) -> EndpointId {
        self.inner.local_endpoint()
    }

    /// A typed handle for sending messages of type `M` to endpoint `dst`.
    pub fn endpoint<M: ChannelMessage>(&self, dst: EndpointId) -> EndpointRef<M> {
        EndpointRef {
            inner: self.inner.clone(),
            dst,
            _marker: PhantomData,
        }
    }

    /// Registers every handler of `router` on this channel, rolling back on
    /// the first conflicting kind. The returned handle keeps the
    /// registration alive.
    pub fn serve(&self, router: ServiceRouter) -> RS<ServiceHandle> {
        let mut kinds = Vec::with_capacity(router.handlers.len());
        for (kind, entry) in router.handlers {
            let inner = self.inner.clone();
            let handler: KindHandler = Arc::new(move |envelope| entry(&inner, envelope));
            if let Err(err) = self.inner.register_kind_handler(kind, handler) {
                for registered in &kinds {
                    let _ = self.inner.unregister_kind_handler(*registered);
                }
                return Err(err);
            }
            kinds.push(kind);
        }
        Ok(ServiceHandle {
            inner: self.inner.clone(),
            kinds,
        })
    }
}

impl<M: ChannelMessage> EndpointRef<M> {
    /// Sends `message` as a request and awaits its typed reply, generating a
    /// fresh idempotency key. Retries of the same logical operation must use
    /// [`EndpointRef::ask_attempt`] to keep the key stable.
    pub async fn ask(&self, message: M, timeout: Duration) -> RS<M::Reply> {
        self.ask_attempt(message, gen_request_id(), 0, timeout)
            .await
    }

    /// One attempt of a request under an explicit idempotency key; retry by
    /// calling again with the same `request_id` and a bumped `attempt`.
    pub async fn ask_attempt(
        &self,
        message: M,
        request_id: RequestId,
        attempt: u32,
        timeout: Duration,
    ) -> RS<M::Reply> {
        let payload = message.encode()?;
        let deadline_ms =
            channel_now_ms().saturating_add(timeout.as_millis().min(u64::MAX as u128) as u64);
        let outgoing = OutgoingMessage::new(M::KIND, payload)
            .with_delivery(DeliveryMode::Request)
            .with_request_id(request_id)
            .with_attempt(attempt)
            .with_deadline_ms(deadline_ms);
        self.inner.send(self.dst, outgoing).await?;
        let filter = RecvFilter {
            src: Some(self.dst),
            dst: Some(self.inner.local_endpoint()),
            kind: Some(M::KIND),
            correlation_id: Some(request_id),
            delivery: Some(DeliveryMode::Response),
        };
        let envelope =
            match mudu_sys::timeout(timeout, self.inner.recv(filter, Some(deadline_ms))).await {
                Some(result) => result?,
                None => return Err(ask_timeout_error::<M>(self.dst, request_id, attempt)),
            };
        match envelope.status() {
            ReplyStatus::Ok => M::Reply::decode(envelope.payload()),
            ReplyStatus::Nack => Err(mudu_error!(
                ErrorCode::Internal,
                format!(
                    "channel request nacked by endpoint {}: {}",
                    self.dst,
                    String::from_utf8_lossy(envelope.payload())
                )
            )),
        }
    }
}

impl<M: ChannelMessage<Reply = ()>> EndpointRef<M> {
    /// Sends `message` without awaiting a reply.
    pub async fn tell(&self, message: M) -> RS<()> {
        let payload = message.encode()?;
        self.inner
            .send(
                self.dst,
                OutgoingMessage::new(M::KIND, payload)
                    .with_delivery(DeliveryMode::FireAndForget)
                    .with_request_id(gen_request_id()),
            )
            .await?;
        Ok(())
    }
}

impl ServiceRouter {
    /// An empty router.
    pub fn new() -> Self {
        Self {
            handlers: HashMap::new(),
        }
    }

    /// Registers `f` as the handler of message type `M`; fails with
    /// `ObjectAlreadyExists` when the kind is already taken on this router.
    ///
    /// The handler returns the reply (or an error, which nacks the
    /// request); responses are sent automatically, fire-and-forget messages
    /// get none.
    pub fn handle<M, F, Fut>(mut self, f: F) -> RS<Self>
    where
        M: ChannelMessage,
        F: Fn(M, ServiceCtx) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = RS<M::Reply>> + 'static,
    {
        if self.handlers.contains_key(&M::KIND) {
            return Err(mudu_error!(
                ErrorCode::ObjectAlreadyExists,
                format!(
                    "message kind {:?} already registered on this router",
                    M::KIND
                )
            ));
        }
        let f = Arc::new(f);
        let entry: RouterEntry = Arc::new(move |channel, envelope| {
            let f = f.clone();
            let channel = channel.clone();
            let src = *envelope.src();
            let kind = envelope.kind();
            let request_id = envelope.request_id();
            let delivery = envelope.delivery();
            let ctx = ServiceCtx::new(src, request_id, envelope.attempt());
            let decoded = M::decode(envelope.payload());
            Box::pin(async move {
                let message = match decoded {
                    Ok(message) => message,
                    Err(err) => {
                        if delivery == DeliveryMode::Request {
                            let _ =
                                reply_nack(&channel, src, kind, request_id, err.to_string()).await;
                        }
                        return Err(err);
                    }
                };
                let outcome = f(message, ctx).await;
                if delivery != DeliveryMode::Request {
                    return outcome.map(|_| ());
                }
                match outcome {
                    Ok(reply) => {
                        let payload = reply.encode()?;
                        channel
                            .send(
                                src,
                                OutgoingMessage::new(kind, payload)
                                    .with_delivery(DeliveryMode::Response)
                                    .with_request_id(request_id)
                                    .with_correlation_id(request_id)
                                    .with_status(ReplyStatus::Ok),
                            )
                            .await?;
                        Ok(())
                    }
                    Err(err) => {
                        let _ = reply_nack(&channel, src, kind, request_id, err.to_string()).await;
                        Err(err)
                    }
                }
            })
        });
        self.handlers.insert(M::KIND, entry);
        Ok(self)
    }
}

impl Default for ServiceRouter {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for ServiceHandle {
    fn drop(&mut self) {
        for kind in self.kinds.drain(..) {
            let _ = self.inner.unregister_kind_handler(kind);
        }
    }
}

fn ask_timeout_error<M: ChannelMessage>(
    dst: EndpointId,
    request_id: RequestId,
    attempt: u32,
) -> mudu::error::MuduError {
    mudu_error!(
        ErrorCode::Tokio,
        format!(
            "channel ask timeout: kind {:?}, dst {}, request_id {}, attempt {}",
            M::KIND,
            dst,
            request_id,
            attempt
        )
    )
}

async fn reply_nack(
    channel: &ChannelEndpointRef,
    dst: EndpointId,
    kind: MessageKind,
    request_id: RequestId,
    message: String,
) -> RS<()> {
    channel
        .send(
            dst,
            OutgoingMessage::new(kind, message.into_bytes())
                .with_delivery(DeliveryMode::Response)
                .with_request_id(request_id)
                .with_correlation_id(request_id)
                .with_status(ReplyStatus::Nack),
        )
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use crate::channel::endpoint::ChannelEndpoint;
    use async_trait::async_trait;
    use mudu::channel::MessageId;
    use mudu_sys::sync::SMutex;
    use serde::{Deserialize, Serialize};
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicU64, Ordering};

    // Test-local kind namespace: 0x7Fxx.
    const TEST_ECHO_KIND: MessageKind = MessageKind::User(0x7F01);
    const TEST_NOTE_KIND: MessageKind = MessageKind::User(0x7F02);
    const TEST_HANG_KIND: MessageKind = MessageKind::User(0x7F03);
    const TEST_UNKNOWN_KIND: MessageKind = MessageKind::User(0x7F7F);

    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
    struct EchoMsg(String);
    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
    struct EchoReply(String);
    mudu::impl_channel_message!(EchoMsg, reply = EchoReply, kind = TEST_ECHO_KIND);

    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
    struct NoteMsg(String);
    mudu::impl_channel_message!(NoteMsg, tell_only, kind = TEST_NOTE_KIND);

    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
    struct HangMsg;
    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
    struct HangReply;
    mudu::impl_channel_message!(HangMsg, reply = HangReply, kind = TEST_HANG_KIND);

    /// Encodes to bytes that no EchoMsg decodes from, sharing its kind.
    struct BadEcho;
    impl ChannelMessage for BadEcho {
        type Reply = EchoReply;
        const KIND: MessageKind = TEST_ECHO_KIND;
        fn encode(&self) -> RS<Vec<u8>> {
            Ok(b"\xff\xfe not messagepack".to_vec())
        }
        fn decode(_bytes: &[u8]) -> RS<Self> {
            unreachable!()
        }
    }

    /// Minimal in-process channel pair for the typed-layer tests: two
    /// endpoints delivering to each other through a shared hub.
    struct TestHub {
        endpoints: SMutex<HashMap<EndpointId, Arc<TestEndpoint>>>,
    }

    struct TestEndpoint {
        local: EndpointId,
        // Weak to break the TestHub <-> TestEndpoint reference cycle;
        // tests keep the hub alive for the whole scenario.
        hub: std::sync::Weak<TestHub>,
        next_msg_id: AtomicU64,
        state: SMutex<TestEndpointState>,
    }

    #[derive(Default)]
    struct TestEndpointState {
        inbox: VecDeque<Envelope>,
        waiters: VecDeque<(RecvFilter, mudu_sys::tokio::sync::oneshot::Sender<Envelope>)>,
        handlers: HashMap<MessageKind, KindHandler>,
    }

    enum TestAction {
        Notify(mudu_sys::tokio::sync::oneshot::Sender<Envelope>, Envelope),
        Dispatch(KindHandler, Envelope),
        Nack(Envelope),
        Drop,
    }

    impl TestHub {
        fn new(ids: &[EndpointId]) -> Arc<Self> {
            let hub = Arc::new(TestHub {
                endpoints: SMutex::new(HashMap::new()),
            });
            for &id in ids {
                let endpoint = Arc::new(TestEndpoint {
                    local: id,
                    hub: Arc::downgrade(&hub),
                    next_msg_id: AtomicU64::new(1),
                    state: SMutex::new(TestEndpointState::default()),
                });
                hub.endpoints.lock().unwrap().insert(id, endpoint);
            }
            hub
        }

        fn channel(&self, id: EndpointId) -> Channel {
            let endpoint = self.endpoints.lock().unwrap().get(&id).unwrap().clone();
            Channel::new(endpoint)
        }
    }

    impl TestEndpoint {
        fn hub(&self) -> Arc<TestHub> {
            self.hub.upgrade().expect("test hub outlives its endpoints")
        }

        fn deliver(&self, envelope: Envelope) {
            let action = {
                let mut state = self.state.lock().unwrap();
                if envelope.is_expired_at(channel_now_ms()) {
                    TestAction::Drop
                } else {
                    match envelope.delivery() {
                        DeliveryMode::Response => {
                            match state
                                .waiters
                                .iter()
                                .position(|(filter, _)| envelope.matches(filter))
                            {
                                Some(index) => {
                                    let (_, sender) = state.waiters.remove(index).unwrap();
                                    TestAction::Notify(sender, envelope)
                                }
                                None => {
                                    state.inbox.push_back(envelope);
                                    TestAction::Drop
                                }
                            }
                        }
                        DeliveryMode::Request | DeliveryMode::FireAndForget => {
                            match state.handlers.get(&envelope.kind()) {
                                Some(handler) => TestAction::Dispatch(handler.clone(), envelope),
                                None if envelope.delivery() == DeliveryMode::Request => {
                                    TestAction::Nack(envelope)
                                }
                                None => TestAction::Drop,
                            }
                        }
                    }
                }
            };
            match action {
                TestAction::Notify(sender, envelope) => {
                    let _ = sender.send(envelope);
                }
                TestAction::Dispatch(handler, envelope) => {
                    let _ = crate::task_async::spawn_local_detached(
                        "test-channel-handler",
                        handler(envelope),
                    );
                }
                TestAction::Nack(request) => {
                    let nack = Envelope::new(
                        self.next_msg_id.fetch_add(1, Ordering::Relaxed),
                        Some(request.request_id()),
                        self.local,
                        *request.src(),
                        request.kind(),
                        format!("no handler registered for kind {:?}", request.kind()).into_bytes(),
                        DeliveryMode::Response,
                    )
                    .with_request_id(request.request_id())
                    .with_status(ReplyStatus::Nack);
                    let src = *nack.dst();
                    let peer = self.hub().endpoints.lock().unwrap().get(&src).cloned();
                    if let Some(peer) = peer {
                        peer.deliver(nack);
                    }
                }
                TestAction::Drop => {}
            }
        }
    }

    #[async_trait]
    impl ChannelEndpoint for TestEndpoint {
        fn local_endpoint(&self) -> EndpointId {
            self.local
        }

        async fn send(&self, dst: EndpointId, message: OutgoingMessage) -> RS<MessageId> {
            if let Some(deadline) = message.deadline_ms()
                && channel_now_ms() >= deadline
            {
                return Err(mudu_error!(
                    ErrorCode::Tokio,
                    "test channel deadline expired"
                ));
            }
            let msg_id = self.next_msg_id.fetch_add(1, Ordering::Relaxed);
            let mut envelope = Envelope::new(
                msg_id,
                message.correlation_id(),
                self.local,
                dst,
                message.kind(),
                message.payload_owned(),
                message.delivery(),
            )
            .with_request_id(message.request_id())
            .with_attempt(message.attempt())
            .with_status(message.status());
            if let Some(deadline) = message.deadline_ms() {
                envelope = envelope.with_deadline_ms(deadline);
            }
            let peer = self.hub().endpoints.lock().unwrap().get(&dst).cloned();
            let peer = peer.ok_or_else(|| {
                mudu_error!(
                    ErrorCode::ObjectNotFound,
                    format!("no such endpoint {}", dst)
                )
            })?;
            peer.deliver(envelope);
            Ok(msg_id)
        }

        async fn recv(&self, filter: RecvFilter, _deadline_ms: Option<u64>) -> RS<Envelope> {
            let receiver = {
                let mut state = self.state.lock().unwrap();
                let index = state.inbox.iter().position(|e| e.matches(&filter));
                match index {
                    Some(index) => return Ok(state.inbox.remove(index).unwrap()),
                    None => {
                        let (sender, receiver) = mudu_sys::tokio::sync::oneshot::channel();
                        state.waiters.push_back((filter, sender));
                        receiver
                    }
                }
            };
            receiver
                .await
                .map_err(|_| mudu_error!(ErrorCode::ChannelClosed, "test channel waiter dropped"))
        }

        fn register_kind_handler(&self, kind: MessageKind, handler: KindHandler) -> RS<()> {
            let mut state = self.state.lock().unwrap();
            if state.handlers.contains_key(&kind) {
                return Err(mudu_error!(
                    ErrorCode::ObjectAlreadyExists,
                    format!("kind {:?} already registered", kind)
                ));
            }
            state.handlers.insert(kind, handler);
            Ok(())
        }

        fn unregister_kind_handler(&self, kind: MessageKind) -> RS<()> {
            self.state.lock().unwrap().handlers.remove(&kind);
            Ok(())
        }
    }

    fn run<F: Future + 'static>(fut: F) -> F::Output
    where
        F::Output: 'static,
    {
        mudu_sys::task::async_::block_on_tokio_current_thread(fut).unwrap()
    }

    #[test]
    fn ask_roundtrip_returns_typed_reply() {
        run(async {
            let hub = TestHub::new(&[1, 2]);
            let server = hub.channel(2);
            let handle = server
                .serve(
                    ServiceRouter::new()
                        .handle::<EchoMsg, _, _>(|msg, _ctx| async move {
                            Ok(EchoReply(format!("{}!", msg.0)))
                        })
                        .unwrap(),
                )
                .unwrap();
            let client = hub.channel(1);
            let reply = client
                .endpoint::<EchoMsg>(2)
                .ask(EchoMsg("hi".to_string()), Duration::from_secs(5))
                .await
                .unwrap();
            assert_eq!(reply, EchoReply("hi!".to_string()));
            drop(handle);
        });
    }

    #[test]
    fn tell_delivers_fire_and_forget() {
        run(async {
            let hub = TestHub::new(&[1, 2]);
            let seen = Arc::new(SMutex::new(Vec::new()));
            let seen2 = seen.clone();
            let server = hub.channel(2);
            let _handle = server
                .serve(
                    ServiceRouter::new()
                        .handle::<NoteMsg, _, _>(move |msg, _ctx| {
                            let seen = seen2.clone();
                            async move {
                                seen.lock().unwrap().push(msg.0);
                                Ok(())
                            }
                        })
                        .unwrap(),
                )
                .unwrap();
            let client = hub.channel(1);
            client
                .endpoint::<NoteMsg>(2)
                .tell(NoteMsg("hello".to_string()))
                .await
                .unwrap();
            for _ in 0..100 {
                if !seen.lock().unwrap().is_empty() {
                    break;
                }
                mudu_sys::tokio::task::yield_now().await;
            }
            assert_eq!(seen.lock().unwrap().as_slice(), ["hello".to_string()]);
        });
    }

    #[test]
    fn duplicate_kind_registration_fails() {
        run(async {
            let hub = TestHub::new(&[1]);
            let server = hub.channel(1);
            let router = ServiceRouter::new()
                .handle::<EchoMsg, _, _>(|msg, _ctx| async move { Ok(EchoReply(msg.0)) })
                .unwrap();
            let result =
                router.handle::<EchoMsg, _, _>(|msg, _ctx| async move { Ok(EchoReply(msg.0)) });
            assert_eq!(result.err().unwrap().ec(), ErrorCode::ObjectAlreadyExists);

            let first = server
                .serve(
                    ServiceRouter::new()
                        .handle::<EchoMsg, _, _>(|msg, _ctx| async move { Ok(EchoReply(msg.0)) })
                        .unwrap(),
                )
                .unwrap();
            let second = server.serve(
                ServiceRouter::new()
                    .handle::<EchoMsg, _, _>(|msg, _ctx| async move { Ok(EchoReply(msg.0)) })
                    .unwrap(),
            );
            assert_eq!(second.err().unwrap().ec(), ErrorCode::ObjectAlreadyExists);
            // The surviving registration still serves.
            let reply = server
                .endpoint::<EchoMsg>(1)
                .ask(EchoMsg("x".to_string()), Duration::from_secs(5))
                .await
                .unwrap();
            assert_eq!(reply, EchoReply("x".to_string()));
            drop(first);
        });
    }

    #[test]
    fn handler_error_nacks_the_request() {
        run(async {
            let hub = TestHub::new(&[1, 2]);
            let server = hub.channel(2);
            let _handle = server
                .serve(
                    ServiceRouter::new()
                        .handle::<EchoMsg, _, _>(|_msg, _ctx| async move {
                            Err::<EchoReply, _>(mudu_error!(ErrorCode::Internal, "boom"))
                        })
                        .unwrap(),
                )
                .unwrap();
            let client = hub.channel(1);
            let err = client
                .endpoint::<EchoMsg>(2)
                .ask(EchoMsg("hi".to_string()), Duration::from_secs(5))
                .await
                .unwrap_err();
            assert_eq!(err.ec(), ErrorCode::Internal);
            assert!(err.to_string().contains("boom"));
        });
    }

    #[test]
    fn decode_failure_nacks_the_request() {
        run(async {
            let hub = TestHub::new(&[1, 2]);
            let server = hub.channel(2);
            let _handle = server
                .serve(
                    ServiceRouter::new()
                        .handle::<EchoMsg, _, _>(|msg, _ctx| async move { Ok(EchoReply(msg.0)) })
                        .unwrap(),
                )
                .unwrap();
            let client = hub.channel(1);
            let err = client
                .endpoint::<BadEcho>(2)
                .ask(BadEcho, Duration::from_secs(5))
                .await
                .unwrap_err();
            assert_eq!(err.ec(), ErrorCode::Internal);
            assert!(err.to_string().contains("decode channel message error"));
        });
    }

    #[test]
    fn unknown_kind_request_nacks() {
        run(async {
            let hub = TestHub::new(&[1, 2]);
            let client = hub.channel(1);
            let err = client
                .endpoint::<EchoMsg>(2)
                .ask(EchoMsg("hi".to_string()), Duration::from_secs(5))
                .await
                .unwrap_err();
            assert_eq!(err.ec(), ErrorCode::Internal);
            assert!(err.to_string().contains("no handler registered"));
        });
    }

    #[test]
    fn ask_times_out_when_never_answered() {
        run(async {
            let hub = TestHub::new(&[1, 2]);
            let server = hub.channel(2);
            let _handle = server
                .serve(
                    ServiceRouter::new()
                        .handle::<HangMsg, _, _>(|_msg, _ctx| async move {
                            std::future::pending::<RS<HangReply>>().await
                        })
                        .unwrap(),
                )
                .unwrap();
            let client = hub.channel(1);
            let err = client
                .endpoint::<HangMsg>(2)
                .ask(HangMsg, Duration::from_millis(30))
                .await
                .unwrap_err();
            assert_eq!(err.ec(), ErrorCode::Tokio);
            assert!(err.to_string().contains("timeout"));
        });
    }

    #[test]
    fn expired_deadline_fails_send() {
        run(async {
            let hub = TestHub::new(&[1, 2]);
            let client = hub.channel(1);
            let err = client
                .raw()
                .send(
                    2,
                    OutgoingMessage::new(TEST_UNKNOWN_KIND, vec![])
                        .with_delivery(DeliveryMode::Request)
                        .with_request_id(1)
                        .with_deadline_ms(1),
                )
                .await
                .unwrap_err();
            assert_eq!(err.ec(), ErrorCode::Tokio);
        });
    }

    #[test]
    fn service_handle_drop_unregisters() {
        run(async {
            let hub = TestHub::new(&[1, 2]);
            let server = hub.channel(2);
            let client = hub.channel(1);
            let reply = {
                let handle = server
                    .serve(
                        ServiceRouter::new()
                            .handle::<EchoMsg, _, _>(
                                |msg, _ctx| async move { Ok(EchoReply(msg.0)) },
                            )
                            .unwrap(),
                    )
                    .unwrap();
                let reply = client
                    .endpoint::<EchoMsg>(2)
                    .ask(EchoMsg("a".to_string()), Duration::from_secs(5))
                    .await
                    .unwrap();
                drop(handle);
                reply
            };
            assert_eq!(reply, EchoReply("a".to_string()));
            // After the handle dropped, the kind is gone: new asks nack.
            let err = client
                .endpoint::<EchoMsg>(2)
                .ask(EchoMsg("b".to_string()), Duration::from_secs(5))
                .await
                .unwrap_err();
            assert!(err.to_string().contains("no handler registered"));
        });
    }
}
