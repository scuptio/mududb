//! Ad-hoc channel endpoints for non-worker threads.
//!
//! Workers route envelopes through their mailbox matrix (keyed by the
//! worker registry); an endpoint living on a non-worker thread (e.g. the
//! management gateway shared by the HTTP management handlers) has no
//! mailbox entry. `LocalEndpoint` provides such an endpoint: outbound
//! envelopes go through a backend-supplied `route` closure (worker mailbox
//! dispatch) with a registry fallback (`deliver_incoming`) for non-worker
//! destinations; inbound envelopes arrive through the same fallback and
//! land directly in its reception state.

use crate::worker_channel_state::{IncomingDispatch, WorkerChannelState};
use async_trait::async_trait;
use mudu::channel::{
    DeliveryMode, EndpointId, Envelope, MessageId, MessageKind, OutgoingMessage, RecvFilter,
    ReplyStatus, ServerInstanceId,
};
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_sys::sync::SMutex;
use mudu_utils::channel::{
    channel_for, channel_now_ms, ChannelEndpoint, ChannelEndpointRef, KindHandler,
};
use mudu_utils::task_async::spawn_local_detached;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Routes an envelope into the backend's worker mailbox matrix; returns
/// `Ok(false)` when the destination is not a worker of this server, letting
/// the caller fall back to registry delivery.
pub(crate) type WorkerRoute = Arc<dyn Fn(Envelope) -> RS<bool> + Send + Sync>;

/// A lightweight [`ChannelEndpoint`] for threads outside the worker set.
///
/// Reception reuses [`WorkerChannelState`]: bounded response buffering,
/// deadline eviction and waiter wakeup behave exactly as on worker
/// endpoints. The endpoint registers no kind handlers by itself; handlers
/// may still be registered through the typed layer (`Channel::serve`).
pub(crate) struct LocalEndpoint {
    local: EndpointId,
    server_instance_id: ServerInstanceId,
    route: WorkerRoute,
    state: SMutex<WorkerChannelState>,
    next_msg_id: AtomicU64,
}

impl LocalEndpoint {
    pub(crate) fn new(
        local: EndpointId,
        server_instance_id: ServerInstanceId,
        route: WorkerRoute,
    ) -> Arc<Self> {
        Arc::new(Self {
            local,
            server_instance_id,
            route,
            state: SMutex::new(WorkerChannelState::new()),
            next_msg_id: AtomicU64::new(1),
        })
    }

    pub(crate) fn channel_ref(self: &Arc<Self>) -> ChannelEndpointRef {
        self.clone()
    }

    /// Sends an envelope: worker destinations go to the mailbox matrix,
    /// everything else is delivered through the channel registry.
    fn route_or_deliver(&self, envelope: Envelope) -> RS<()> {
        if !(self.route)(envelope.clone())? {
            let endpoint = channel_for(self.server_instance_id, *envelope.dst())?;
            endpoint.deliver_incoming(envelope)?;
        }
        Ok(())
    }

    fn lock_state(&self) -> RS<mudu_sys::sync::SMutexGuard<'_, WorkerChannelState>> {
        self.state
            .lock()
            .map_err(|_| mudu_error!(ErrorCode::Internal, "local endpoint state lock poisoned"))
    }
}

#[async_trait]
impl ChannelEndpoint for LocalEndpoint {
    fn local_endpoint(&self) -> EndpointId {
        self.local
    }

    async fn send(&self, dst: EndpointId, message: OutgoingMessage) -> RS<MessageId> {
        if let Some(deadline) = message.deadline_ms() {
            if channel_now_ms() >= deadline {
                return Err(mudu_error!(
                    ErrorCode::Tokio,
                    format!(
                        "channel message deadline expired before send: kind {:?}",
                        message.kind()
                    )
                ));
            }
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
        self.route_or_deliver(envelope)?;
        Ok(msg_id)
    }

    async fn recv(&self, filter: RecvFilter, deadline_ms: Option<u64>) -> RS<Envelope> {
        let receiver = {
            let mut state = self.lock_state()?;
            if let Some(envelope) = state.try_take_message(&filter) {
                return Ok(envelope);
            }
            state.register_waiter(filter, deadline_ms)
        };
        receiver.wait().await?.ok_or_else(|| {
            mudu_error!(
                ErrorCode::ChannelClosed,
                "local endpoint waiter dropped before delivery"
            )
        })
    }

    fn register_kind_handler(&self, kind: MessageKind, handler: KindHandler) -> RS<()> {
        self.lock_state()?.register_kind_handler(kind, handler)
    }

    fn unregister_kind_handler(&self, kind: MessageKind) -> RS<()> {
        self.lock_state()?.unregister_kind_handler(kind)
    }

    fn deliver_incoming(&self, envelope: Envelope) -> RS<()> {
        let dispatch = self.lock_state()?.handle_incoming(envelope);
        match dispatch {
            // The caller of deliver_incoming is another endpoint's send path,
            // which runs on a worker thread (a LocalSet is active there).
            IncomingDispatch::Dispatch(handler, envelope) => {
                let _ = spawn_local_detached("local_endpoint_handler", async move {
                    let _ = handler(envelope).await;
                });
            }
            IncomingDispatch::NackUnknown(request) => {
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
                // Best-effort: the asker times out when the nack itself fails.
                let _ = self.route_or_deliver(nack);
            }
            IncomingDispatch::Delivered | IncomingDispatch::Buffered => {}
        }
        Ok(())
    }

    fn close(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.close();
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use crate::server::worker_channel::TokioWorkerChannel;
    use crate::worker_registry::{WorkerIdobject, WorkerRegistry};
    use crossbeam_queue::SegQueue;
    use mudu_sys::sync::async_::notify_wait::create_notify_wait;
    use mudu_sys::tokio::sync::Notify;
    use mudu_utils::channel::{spawn_endpoint, EndpointGuard};

    // Each test uses its own server instance id: the channel registry is a
    // process-global table shared with the other tests running in parallel.
    const GW: EndpointId = 0x6D67_6D74_0000_0000_0000_0000_0000_0001;
    const KIND: MessageKind = MessageKind::User(0x7F21);

    struct Fabric {
        registry: Arc<WorkerRegistry>,
        mailboxes: Vec<Arc<SegQueue<Envelope>>>,
        wakes: Vec<Arc<Notify>>,
    }

    fn test_fabric() -> Fabric {
        let registry = Arc::new(
            WorkerRegistry::new(vec![
                WorkerIdobject {
                    worker_index: 0,
                    worker_id: 11,
                    partition_ids: vec![101],
                },
                WorkerIdobject {
                    worker_index: 1,
                    worker_id: 12,
                    partition_ids: vec![102],
                },
            ])
            .unwrap(),
        );
        Fabric {
            registry,
            mailboxes: vec![Arc::new(SegQueue::new()), Arc::new(SegQueue::new())],
            wakes: vec![Arc::new(Notify::new()), Arc::new(Notify::new())],
        }
    }

    fn worker_route(fabric: &Fabric) -> WorkerRoute {
        let registry = fabric.registry.clone();
        let mailboxes = fabric.mailboxes.clone();
        let wakes = fabric.wakes.clone();
        Arc::new(move |envelope: Envelope| {
            match registry.worker_index_by_worker_id(*envelope.dst()) {
                Some(index) => {
                    mailboxes[index].push(envelope);
                    wakes[index].notify_one();
                    Ok(true)
                }
                None => Ok(false),
            }
        })
    }

    fn gateway(fabric: &Fabric, sid: ServerInstanceId) -> (Arc<LocalEndpoint>, EndpointGuard) {
        let endpoint = LocalEndpoint::new(GW, sid, worker_route(fabric));
        let (_channel, guard) = spawn_endpoint(sid, GW, &endpoint.channel_ref()).unwrap();
        (endpoint, guard)
    }

    #[test]
    fn worker_to_gateway_response_delivers_through_registry_fallback() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let fabric = test_fabric();
            let sid = 101;
            let worker = TokioWorkerChannel::new(
                11,
                sid,
                fabric.registry.clone(),
                fabric.mailboxes.clone(),
                fabric.wakes.clone(),
                Arc::new(SMutex::new(None)),
            );
            let (gateway, guard) = gateway(&fabric, sid);

            worker
                .send(
                    GW,
                    OutgoingMessage::new(KIND, b"reply".to_vec())
                        .with_delivery(DeliveryMode::Response)
                        .with_request_id(5)
                        .with_correlation_id(5),
                )
                .await
                .unwrap();
            let envelope = gateway
                .recv(
                    RecvFilter {
                        correlation_id: Some(5),
                        delivery: Some(DeliveryMode::Response),
                        ..RecvFilter::default()
                    },
                    None,
                )
                .await
                .unwrap();
            assert_eq!(envelope.payload(), b"reply");
            assert_eq!(*envelope.src(), 11);

            // The registry fallback errors cleanly for unknown endpoints,
            // and for the gateway once its guard dropped the registration.
            let err = worker
                .send(777, OutgoingMessage::new(KIND, vec![]))
                .await
                .unwrap_err();
            assert_eq!(err.ec(), ErrorCode::ObjectNotFound);
            drop(guard);
            let err = worker
                .send(GW, OutgoingMessage::new(KIND, vec![]))
                .await
                .unwrap_err();
            assert_eq!(err.ec(), ErrorCode::ObjectNotFound);
        })
        .unwrap()
    }

    #[test]
    fn gateway_send_reaches_the_worker_mailbox() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let fabric = test_fabric();
            let (gateway, _guard) = gateway(&fabric, 102);
            gateway
                .send(
                    12,
                    OutgoingMessage::new(KIND, b"ask".to_vec())
                        .with_delivery(DeliveryMode::Request)
                        .with_request_id(9),
                )
                .await
                .unwrap();
            let envelope = fabric.mailboxes[1].pop().unwrap();
            assert_eq!(*envelope.dst(), 12);
            assert_eq!(*envelope.src(), GW);
            assert_eq!(envelope.payload(), b"ask");
            assert_eq!(envelope.delivery(), DeliveryMode::Request);
        })
        .unwrap()
    }

    #[test]
    fn gateway_unknown_kind_request_nacks_back() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let fabric = test_fabric();
            let (gateway, _guard) = gateway(&fabric, 103);
            // A request arrives at the gateway from worker 11; the nack is
            // routed back through the route closure into the worker mailbox.
            gateway
                .deliver_incoming(
                    Envelope::new(1, None, 11, GW, KIND, vec![], DeliveryMode::Request)
                        .with_request_id(42),
                )
                .unwrap();
            let nack = fabric.mailboxes[0].pop().unwrap();
            assert_eq!(nack.status(), ReplyStatus::Nack);
            assert_eq!(nack.correlation_id(), Some(42));
            assert_eq!(*nack.dst(), 11);
        })
        .unwrap()
    }

    #[test]
    fn close_wakes_a_pending_gateway_recv() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let fabric = test_fabric();
            let (gateway, _guard) = gateway(&fabric, 104);
            let (sender, receiver) = create_notify_wait();
            let recv_gateway = gateway.clone();
            let _task = spawn_local_detached("test-gw-recv", async move {
                let result = recv_gateway.recv(RecvFilter::default(), None).await;
                let _ = sender.notify(result.map(|_| ()));
            });
            for _ in 0..100 {
                if gateway.state.lock().unwrap().waiter_count() > 0 {
                    break;
                }
                mudu_sys::tokio::task::yield_now().await;
            }
            gateway.close();
            let result = receiver.wait().await.unwrap().expect("recv finished");
            assert_eq!(result.unwrap_err().ec(), ErrorCode::ChannelClosed);
        })
        .unwrap()
    }
}
