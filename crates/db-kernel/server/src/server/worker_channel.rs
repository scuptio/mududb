use super::*;

pub(crate) struct TokioWorkerChannel {
    local_worker_id: OID,
    server_instance_id: ServerInstanceId,
    registry: Arc<WorkerRegistry>,
    mailboxes: Vec<Arc<SegQueue<Envelope>>>,
    mailbox_wakes: Vec<Arc<Notify>>,
    next_msg_id: AtomicU64,
    state: SMutex<WorkerChannelState>,
}

impl TokioWorkerChannel {
    pub(crate) fn new(
        local_worker_id: OID,
        server_instance_id: ServerInstanceId,
        registry: Arc<WorkerRegistry>,
        mailboxes: Vec<Arc<SegQueue<Envelope>>>,
        mailbox_wakes: Vec<Arc<Notify>>,
    ) -> Arc<Self> {
        Arc::new(Self {
            local_worker_id,
            server_instance_id,
            registry,
            mailboxes,
            mailbox_wakes,
            next_msg_id: AtomicU64::new(1),
            state: SMutex::new(WorkerChannelState::new()),
        })
    }

    pub(crate) fn channel_ref(self: &Arc<Self>) -> ChannelEndpointRef {
        self.clone()
    }

    pub(crate) fn handle_incoming(&self, envelope: Envelope) -> RS<()> {
        let dispatch = {
            let mut state = self.state.lock().map_err(|_| {
                mudu_error!(
                    ErrorCode::Internal,
                    "tokio worker channel state lock poisoned"
                )
            })?;
            state.handle_incoming(envelope)
        };
        match dispatch {
            IncomingDispatch::Dispatch(handler, envelope) => {
                let _ = spawn_local_detached("worker_channel_handler", async move {
                    let _ = handler(envelope).await;
                });
            }
            IncomingDispatch::NackUnknown(request) => {
                // Best-effort: the asker times out when the nack itself fails.
                let nack = self.nack_envelope(
                    &request,
                    format!("no handler registered for kind {:?}", request.kind()),
                );
                let _ = self.enqueue_envelope(nack);
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

    fn enqueue_envelope(&self, envelope: Envelope) -> RS<()> {
        match self.registry.worker_index_by_worker_id(*envelope.dst()) {
            Some(target_worker) => {
                let Some(mailbox) = self.mailboxes.get(target_worker) else {
                    return Err(mudu_error!(
                        ErrorCode::Internal,
                        format!("mailbox target worker {} is out of range", target_worker)
                    ));
                };
                mailbox.push(envelope);
                if let Some(wake) = self.mailbox_wakes.get(target_worker) {
                    wake.notify_one();
                }
                Ok(())
            }
            None => {
                // Not a worker of this server: deliver through the channel
                // registry (e.g. the management gateway endpoint).
                let endpoint = channel_for(self.server_instance_id, *envelope.dst())?;
                endpoint.deliver_incoming(envelope)
            }
        }
    }
}

#[async_trait]
impl ChannelEndpoint for TokioWorkerChannel {
    fn local_endpoint(&self) -> EndpointId {
        self.local_worker_id
    }

    async fn send(&self, dst: EndpointId, message: OutgoingMessage) -> RS<MessageId> {
        scoped_task_trace!();
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
        self.enqueue_envelope(envelope)?;
        Ok(msg_id)
    }

    async fn recv(&self, filter: RecvFilter, deadline_ms: Option<u64>) -> RS<Envelope> {
        let receiver = {
            let mut state = self.state.lock().map_err(|_| {
                mudu_error!(
                    ErrorCode::Internal,
                    "tokio worker channel state lock poisoned"
                )
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
        let mut state = self.state.lock().map_err(|_| {
            mudu_error!(
                ErrorCode::Internal,
                "tokio worker channel state lock poisoned"
            )
        })?;
        state.register_kind_handler(kind, handler)
    }

    fn unregister_kind_handler(&self, kind: MessageKind) -> RS<()> {
        let mut state = self.state.lock().map_err(|_| {
            mudu_error!(
                ErrorCode::Internal,
                "tokio worker channel state lock poisoned"
            )
        })?;
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
        self.enqueue_envelope(envelope)
    }
}

unsafe impl Send for TokioWorkerChannel {}
unsafe impl Sync for TokioWorkerChannel {}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;
    use mudu::channel::ReplyStatus;

    fn test_channels() -> (
        Arc<TokioWorkerChannel>,
        Arc<TokioWorkerChannel>,
        Vec<Arc<SegQueue<Envelope>>>,
    ) {
        let registry = Arc::new(
            WorkerRegistry::new(vec![
                crate::worker_registry::WorkerIdentity {
                    worker_index: 0,
                    worker_id: 11,
                    partition_ids: vec![101],
                },
                crate::worker_registry::WorkerIdentity {
                    worker_index: 1,
                    worker_id: 12,
                    partition_ids: vec![102],
                },
            ])
            .unwrap(),
        );
        let mailboxes: Vec<Arc<SegQueue<Envelope>>> =
            vec![Arc::new(SegQueue::new()), Arc::new(SegQueue::new())];
        let wakes = vec![Arc::new(Notify::new()), Arc::new(Notify::new())];
        let channel0 =
            TokioWorkerChannel::new(11, 1, registry.clone(), mailboxes.clone(), wakes.clone());
        let channel1 = TokioWorkerChannel::new(12, 1, registry, mailboxes.clone(), wakes);
        (channel0, channel1, mailboxes)
    }

    #[test]
    fn send_routes_envelope_to_the_target_mailbox() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let (channel0, _channel1, mailboxes) = test_channels();
            let msg_id = channel0
                .send(
                    12,
                    OutgoingMessage::new(MessageKind::User(7), b"hi".to_vec()),
                )
                .await
                .unwrap();
            assert_eq!(msg_id, 1);
            let envelope = mailboxes[1].pop().unwrap();
            assert_eq!(*envelope.dst(), 12);
            assert_eq!(envelope.payload(), b"hi");
            assert!(mailboxes[0].is_empty());

            let err = channel0
                .send(999_999, OutgoingMessage::new(MessageKind::User(1), vec![]))
                .await
                .unwrap_err();
            assert_eq!(err.ec(), ErrorCode::EntityNotFound);
        })
        .unwrap()
    }

    #[test]
    fn unknown_kind_request_nacks_back_to_the_asker() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let (channel0, channel1, mailboxes) = test_channels();
            channel0
                .send(
                    12,
                    OutgoingMessage::new(MessageKind::User(9), vec![])
                        .with_delivery(DeliveryMode::Request)
                        .with_request_id(77),
                )
                .await
                .unwrap();
            let request = mailboxes[1].pop().unwrap();
            channel1.handle_incoming(request).unwrap();

            // The nack lands in the asker's mailbox, correlated by request id.
            let nack = mailboxes[0].pop().unwrap();
            assert_eq!(nack.status(), ReplyStatus::Nack);
            assert_eq!(nack.correlation_id(), Some(77));
            assert_eq!(nack.kind(), MessageKind::User(9));
            assert_eq!(*nack.src(), 12);

            // The worker loop drains the mailbox into the channel state;
            // then the asker's recv filter matches it.
            channel0.handle_incoming(nack).unwrap();
            let reply = channel0
                .recv(
                    RecvFilter {
                        src: Some(12),
                        kind: Some(MessageKind::User(9)),
                        correlation_id: Some(77),
                        delivery: Some(DeliveryMode::Response),
                        ..RecvFilter::default()
                    },
                    None,
                )
                .await
                .unwrap();
            assert_eq!(reply.status(), ReplyStatus::Nack);
        })
        .unwrap()
    }

    #[test]
    fn request_dispatches_to_the_registered_kind_handler() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let (channel0, channel1, mailboxes) = test_channels();
            let hits = Arc::new(AtomicU64::new(0));
            let hits2 = hits.clone();
            channel1
                .register_kind_handler(
                    MessageKind::User(5),
                    Arc::new(move |envelope| {
                        let hits = hits2.clone();
                        Box::pin(async move {
                            hits.fetch_add(1, Ordering::Relaxed);
                            assert_eq!(envelope.payload(), b"ping");
                            Ok(())
                        })
                    }),
                )
                .unwrap();
            channel0
                .send(
                    12,
                    OutgoingMessage::new(MessageKind::User(5), b"ping".to_vec())
                        .with_delivery(DeliveryMode::Request)
                        .with_request_id(1),
                )
                .await
                .unwrap();
            let request = mailboxes[1].pop().unwrap();
            channel1.handle_incoming(request).unwrap();
            for _ in 0..100 {
                if hits.load(Ordering::Relaxed) == 1 {
                    break;
                }
                mudu_sys::tokio::task::yield_now().await;
            }
            assert_eq!(hits.load(Ordering::Relaxed), 1);

            // Duplicate registration is rejected.
            let err = channel1
                .register_kind_handler(
                    MessageKind::User(5),
                    Arc::new(|_| Box::pin(async { Ok(()) })),
                )
                .unwrap_err();
            assert_eq!(err.ec(), ErrorCode::EntityAlreadyExists);
        })
        .unwrap()
    }

    #[test]
    fn expired_deadline_fails_before_send() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let (channel0, _channel1, mailboxes) = test_channels();
            let err = channel0
                .send(
                    12,
                    OutgoingMessage::new(MessageKind::User(1), vec![]).with_deadline_ms(1),
                )
                .await
                .unwrap_err();
            assert_eq!(err.ec(), ErrorCode::Tokio);
            assert!(mailboxes[1].is_empty());
        })
        .unwrap()
    }

    #[test]
    fn close_wakes_pending_recv_with_channel_closed() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let (channel0, _channel1, _mailboxes) = test_channels();
            let slot = Arc::new(SMutex::new(None));
            let slot2 = slot.clone();
            let channel = channel0.clone();
            let _task = spawn_local_detached("test-recv", async move {
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
