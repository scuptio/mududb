use crate::server_iouring;
use crate::task;
use crate::worker_channel_state::{IncomingDispatch, WorkerChannelState};
use crate::worker_mailbox::WorkerMailboxMsg;
use crate::worker_registry::WorkerRegistry;
use async_trait::async_trait;
use crossbeam_queue::SegQueue;
use mudu::channel::{
    DeliveryMode, EndpointId, Envelope, MessageId, MessageKind, OutgoingMessage, RecvFilter,
    ReplyStatus, ServerInstanceId,
};
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_sys::sync::SMutex;
use mudu_utils::channel::{
    channel_for, channel_now_ms, ChannelEndpoint, ChannelEndpointRef, KindHandler,
};
use std::os::fd::RawFd;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use tracing::debug;

pub(crate) struct UringWorkerChannel {
    local_worker_id: OID,
    server_instance_id: ServerInstanceId,
    registry: Arc<WorkerRegistry>,
    mailbox_fds: Vec<RawFd>,
    mailboxes: Vec<Arc<SegQueue<WorkerMailboxMsg>>>,
    next_msg_id: AtomicU64,
    state: SMutex<WorkerChannelState>,
}

unsafe impl Send for UringWorkerChannel {}
unsafe impl Sync for UringWorkerChannel {}

impl UringWorkerChannel {
    pub fn new(
        local_worker_id: OID,
        server_instance_id: ServerInstanceId,
        registry: Arc<WorkerRegistry>,
        mailbox_fds: Vec<RawFd>,
        mailboxes: Vec<Arc<SegQueue<WorkerMailboxMsg>>>,
    ) -> Arc<Self> {
        Arc::new(Self {
            local_worker_id,
            server_instance_id,
            registry,
            mailbox_fds,
            mailboxes,
            next_msg_id: AtomicU64::new(1),
            state: SMutex::new(WorkerChannelState::new()),
        })
    }

    pub(crate) fn channel_ref(self: &Arc<Self>) -> ChannelEndpointRef {
        self.clone()
    }

    pub(crate) fn handle_incoming(&self, envelope: Envelope) -> RS<()> {
        debug!(
            local_worker_id = self.local_worker_id,
            src = ?envelope.src(),
            dst = ?envelope.dst(),
            kind = ?envelope.kind(),
            msg_id = envelope.msg_id(),
            correlation_id = ?envelope.correlation_id(),
            "worker_channel handle incoming"
        );
        let dispatch = {
            let mut state = self.state.lock().map_err(|_| {
                mudu_error!(ErrorCode::Internal, "worker channel state lock poisoned")
            })?;
            state.handle_incoming(envelope)
        };
        match dispatch {
            IncomingDispatch::Dispatch(handler, envelope) => {
                debug!(
                    local_worker_id = self.local_worker_id,
                    src = ?envelope.src(),
                    dst = ?envelope.dst(),
                    kind = ?envelope.kind(),
                    msg_id = envelope.msg_id(),
                    "worker_channel dispatching handler task"
                );
                let future = handler(envelope);
                task::spawn_system("worker-channel-handler", future);
            }
            IncomingDispatch::NackUnknown(request) => {
                // Best-effort: the asker times out when the nack itself fails.
                let nack = self.nack_envelope(
                    &request,
                    format!("no handler registered for kind {:?}", request.kind()),
                );
                let _ = self.dispatch_mailbox_envelope(nack);
            }
            IncomingDispatch::Delivered | IncomingDispatch::Buffered => {}
        }
        Ok(())
    }

    fn nack_envelope(&self, request: &Envelope, message: String) -> Envelope {
        Envelope::new(
            self.next_msg_id.fetch_add(1, Ordering::Relaxed),
            Some(request.request_id()),
            self.local_worker_id,
            *request.src(),
            request.kind(),
            message.into_bytes(),
            DeliveryMode::Response,
        )
        .with_request_id(request.request_id())
        .with_status(ReplyStatus::Nack)
    }

    fn route_worker_index(&self, endpoint: EndpointId) -> RS<usize> {
        self.registry
            .worker_index_by_worker_id(endpoint)
            .ok_or_else(|| {
                mudu_error!(
                    ErrorCode::EntityNotFound,
                    format!("no such worker id {}", endpoint)
                )
            })
    }

    fn dispatch_mailbox_envelope(&self, envelope: Envelope) -> RS<()> {
        match self.registry.worker_index_by_worker_id(*envelope.dst()) {
            Some(target_worker) => {
                self.dispatch_mailbox_message(target_worker, WorkerMailboxMsg::BusMessage(envelope))
            }
            None => {
                // Not a worker of this server: deliver through the channel
                // registry (e.g. the management gateway endpoint).
                let endpoint = channel_for(self.server_instance_id, *envelope.dst())?;
                endpoint.deliver_incoming(envelope)
            }
        }
    }

    fn dispatch_mailbox_message(&self, target_worker: usize, msg: WorkerMailboxMsg) -> RS<()> {
        let Some(mailbox) = self.mailboxes.get(target_worker) else {
            return Err(mudu_error!(
                ErrorCode::Internal,
                format!("mailbox target worker {} is out of range", target_worker)
            ));
        };
        let Some(&fd) = self.mailbox_fds.get(target_worker) else {
            return Err(mudu_error!(
                ErrorCode::Internal,
                format!(
                    "mailbox eventfd target worker {} is out of range",
                    target_worker
                )
            ));
        };
        debug!(
            local_worker_id = self.local_worker_id,
            target_worker,
            mailbox_fd = fd,
            msg = ?msg,
            "worker_channel enqueue mailbox message"
        );
        mailbox.push(msg);
        server_iouring::notify_mailbox_fd(fd)
    }
}

#[async_trait]
impl ChannelEndpoint for UringWorkerChannel {
    fn local_endpoint(&self) -> EndpointId {
        self.local_worker_id
    }

    async fn send(&self, dst: EndpointId, message: OutgoingMessage) -> RS<MessageId> {
        mudu_utils::scoped_task_trace!();
        if let Some(deadline) = message.deadline_ms() {
            if channel_now_ms() >= deadline {
                return Err(mudu_error!(
                    ErrorCode::Tokio,
                    format!(
                        "worker channel message deadline expired before send: kind {:?}",
                        message.kind()
                    )
                ));
            }
        }
        let msg_id = self.next_msg_id.fetch_add(1, Ordering::Relaxed);
        let mut envelope = Envelope::new(
            msg_id,
            message.correlation_id(),
            self.local_endpoint(),
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
        debug!(
            local_worker_id = self.local_worker_id,
            dst = ?envelope.dst(),
            kind = ?envelope.kind(),
            msg_id,
            correlation_id = ?envelope.correlation_id(),
            "worker_channel send"
        );
        self.dispatch_mailbox_envelope(envelope)?;
        Ok(msg_id)
    }

    async fn recv(&self, filter: RecvFilter, deadline_ms: Option<u64>) -> RS<Envelope> {
        mudu_utils::scoped_task_trace!();
        let receiver = {
            let mut state = self.state.lock().map_err(|_| {
                mudu_error!(ErrorCode::Internal, "worker channel state lock poisoned")
            })?;
            if let Some(envelope) = state.try_take_message(&filter) {
                return Ok(envelope);
            }
            state.register_waiter(filter, deadline_ms)
        };
        receiver.wait().await?.ok_or_else(|| {
            mudu_error!(
                ErrorCode::ChannelClosed,
                "worker channel waiter dropped before delivery"
            )
        })
    }

    fn register_kind_handler(&self, kind: MessageKind, handler: KindHandler) -> RS<()> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| mudu_error!(ErrorCode::Internal, "worker channel state lock poisoned"))?;
        state.register_kind_handler(kind, handler)
    }

    fn unregister_kind_handler(&self, kind: MessageKind) -> RS<()> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| mudu_error!(ErrorCode::Internal, "worker channel state lock poisoned"))?;
        state.unregister_kind_handler(kind)
    }

    fn close(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.close();
        }
    }

    fn deliver_incoming(&self, envelope: Envelope) -> RS<()> {
        // A worker endpoint's incoming path is its own mailbox: the worker
        // loop drains it into the channel state on the worker thread.
        let target_worker = self.route_worker_index(self.local_worker_id)?;
        self.dispatch_mailbox_message(target_worker, WorkerMailboxMsg::BusMessage(envelope))
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use crate::worker_registry::WorkerIdentity;
    use mudu::channel::ReplyStatus;

    fn test_registry() -> Arc<WorkerRegistry> {
        Arc::new(
            WorkerRegistry::new(vec![
                WorkerIdentity {
                    worker_index: 0,
                    worker_id: 11,
                    partition_ids: vec![101],
                },
                WorkerIdentity {
                    worker_index: 1,
                    worker_id: 12,
                    partition_ids: vec![102],
                },
            ])
            .unwrap(),
        )
    }

    fn test_channel(worker_id: OID) -> Arc<UringWorkerChannel> {
        UringWorkerChannel::new(
            worker_id,
            1,
            test_registry(),
            vec![0, 1],
            vec![Arc::new(SegQueue::new()), Arc::new(SegQueue::new())],
        )
    }

    #[test]
    fn recv_consumes_buffered_response() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let channel = test_channel(11);
            channel
                .handle_incoming(
                    Envelope::new(
                        1,
                        Some(9),
                        12,
                        11,
                        MessageKind::User(7),
                        b"ping".to_vec(),
                        DeliveryMode::Response,
                    )
                    .with_request_id(9),
                )
                .unwrap();

            let message = channel
                .recv(
                    RecvFilter {
                        src: Some(12),
                        kind: Some(MessageKind::User(7)),
                        delivery: Some(DeliveryMode::Response),
                        ..RecvFilter::default()
                    },
                    None,
                )
                .await
                .unwrap();
            assert_eq!(message.payload(), b"ping");
        })
        .unwrap()
    }

    #[test]
    fn send_enqueues_mailbox_message_and_wakes_eventfd() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let fd0 = mudu_sys::sync::sync_::blocking::eventfd().unwrap();
            let fd1 = mudu_sys::sync::sync_::blocking::eventfd().unwrap();
            let mailboxes = vec![Arc::new(SegQueue::new()), Arc::new(SegQueue::new())];
            let channel =
                UringWorkerChannel::new(11, 1, test_registry(), vec![fd0, fd1], mailboxes.clone());
            assert_eq!(channel.local_endpoint(), 11);

            let msg_id = channel
                .send(
                    12,
                    OutgoingMessage::new(MessageKind::User(3), b"hello".to_vec())
                        .with_delivery(DeliveryMode::Request)
                        .with_request_id(5),
                )
                .await
                .unwrap();
            assert_eq!(msg_id, 1);
            let Some(WorkerMailboxMsg::BusMessage(envelope)) = mailboxes[1].pop() else {
                panic!("expected a channel message in worker 1's mailbox");
            };
            assert_eq!(envelope.payload(), b"hello");
            assert_eq!(*envelope.src(), 11);
            assert_eq!(envelope.request_id(), 5);
            // The eventfd wake was signaled (8-byte counter read).
            let mut buf = [0u8; 8];
            // SAFETY: fd1 is a live eventfd and buf is an 8-byte buffer.
            let n = unsafe { libc::read(fd1, buf.as_mut_ptr() as *mut libc::c_void, 8) };
            assert_eq!(n, 8);
            let _ = mudu_sys::sync::sync_::blocking::close_fd(fd0);
            let _ = mudu_sys::sync::sync_::blocking::close_fd(fd1);
        })
        .unwrap()
    }

    #[test]
    fn send_rejects_unknown_endpoint() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let channel = test_channel(11);
            let err = channel
                .send(77, OutgoingMessage::new(MessageKind::User(1), vec![]))
                .await
                .unwrap_err();
            assert_eq!(err.ec(), ErrorCode::EntityNotFound);
        })
        .unwrap()
    }

    #[test]
    fn send_reports_mailbox_and_fd_table_mismatch() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            // Registry knows worker 12 (index 1) but the mailbox table is
            // one short: the mismatch is an Internal error, not a panic.
            let channel = UringWorkerChannel::new(
                11,
                1,
                test_registry(),
                vec![0],
                vec![Arc::new(SegQueue::new())],
            );
            let err = channel
                .send(12, OutgoingMessage::new(MessageKind::User(1), vec![]))
                .await
                .unwrap_err();
            assert_eq!(err.ec(), ErrorCode::Internal);

            // Mailbox present but eventfd table short: also Internal.
            let channel = UringWorkerChannel::new(
                11,
                1,
                test_registry(),
                vec![],
                vec![Arc::new(SegQueue::new()), Arc::new(SegQueue::new())],
            );
            let err = channel
                .send(12, OutgoingMessage::new(MessageKind::User(1), vec![]))
                .await
                .unwrap_err();
            assert_eq!(err.ec(), ErrorCode::Internal);
        })
        .unwrap()
    }

    #[test]
    fn unknown_request_nacks_into_the_askers_mailbox() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let mailboxes = vec![Arc::new(SegQueue::new()), Arc::new(SegQueue::new())];
            let channel =
                UringWorkerChannel::new(12, 1, test_registry(), vec![0, 1], mailboxes.clone());
            channel
                .handle_incoming(
                    Envelope::new(
                        1,
                        None,
                        11,
                        12,
                        MessageKind::User(9),
                        vec![],
                        DeliveryMode::Request,
                    )
                    .with_request_id(42),
                )
                .unwrap();
            let Some(WorkerMailboxMsg::BusMessage(nack)) = mailboxes[0].pop() else {
                panic!("expected a nack in the asker's mailbox");
            };
            assert_eq!(nack.status(), ReplyStatus::Nack);
            assert_eq!(nack.correlation_id(), Some(42));
            assert_eq!(nack.kind(), MessageKind::User(9));
            assert_eq!(*nack.src(), 12);
        })
        .unwrap()
    }

    #[test]
    fn kind_handler_registration_is_exclusive() {
        let channel = test_channel(11);
        let handler: KindHandler = Arc::new(|_envelope| Box::pin(async { Ok(()) }));
        channel
            .register_kind_handler(MessageKind::User(1), handler.clone())
            .unwrap();
        let err = channel
            .register_kind_handler(MessageKind::User(1), handler)
            .unwrap_err();
        assert_eq!(err.ec(), ErrorCode::EntityAlreadyExists);
        channel
            .unregister_kind_handler(MessageKind::User(1))
            .unwrap();
        channel
            .register_kind_handler(
                MessageKind::User(1),
                Arc::new(|_e| Box::pin(async { Ok(()) })),
            )
            .unwrap();
    }

    #[test]
    fn close_wakes_pending_recv_with_channel_closed() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let channel0 = test_channel(11);
            let slot = Arc::new(SMutex::new(None));
            let slot2 = slot.clone();
            let channel = channel0.clone();
            let _task = mudu_utils::task_async::spawn_local_detached("test-recv", async move {
                *slot2.lock().unwrap() = Some(channel.recv(RecvFilter::default(), None).await);
            });
            for _ in 0..100 {
                if channel0.state.lock().unwrap().waiter_count() > 0 {
                    break;
                }
                mudu_sys::tokio::task::yield_now().await;
            }
            channel0.close();
            for _ in 0..100 {
                if slot.lock().unwrap().is_some() {
                    break;
                }
                mudu_sys::tokio::task::yield_now().await;
            }
            let result = slot.lock().unwrap().take().expect("recv finished");
            assert_eq!(result.unwrap_err().ec(), ErrorCode::ChannelClosed);
        })
        .unwrap()
    }
}
