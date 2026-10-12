use super::*;

pub(crate) struct TokioWorkerMessageBus {
    local_worker_id: OID,
    registry: Arc<WorkerRegistry>,
    mailboxes: Vec<Arc<SegQueue<Envelope>>>,
    mailbox_wakes: Vec<Arc<Notify>>,
    next_msg_id: AtomicU64,
    state: SMutex<WorkerMessageBusState>,
}

impl TokioWorkerMessageBus {
    pub(crate) fn new(
        local_worker_id: OID,
        registry: Arc<WorkerRegistry>,
        mailboxes: Vec<Arc<SegQueue<Envelope>>>,
        mailbox_wakes: Vec<Arc<Notify>>,
    ) -> Arc<Self> {
        Arc::new(Self {
            local_worker_id,
            registry,
            mailboxes,
            mailbox_wakes,
            next_msg_id: AtomicU64::new(1),
            state: SMutex::new(WorkerMessageBusState::new()),
        })
    }

    pub(crate) fn bus_ref(self: &Arc<Self>) -> MessageBusRef {
        self.clone()
    }

    pub(crate) fn handle_incoming(&self, envelope: Envelope) -> RS<()> {
        let maybe_callback = {
            let mut state = self.state.lock().map_err(|_| {
                mudu_error!(ErrorCode::Internal, "tokio message bus state lock poisoned")
            })?;
            state.handle_incoming(envelope)
        };
        if let Some((callback, envelope)) = maybe_callback {
            let _ = spawn_local_detached("tokio_message_bus_handle_incoming", async move {
                let _ = (callback)(envelope).await;
            });
        }
        Ok(())
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
}

#[async_trait]
impl MessageBus for TokioWorkerMessageBus {
    fn local_endpoint(&self) -> EndpointId {
        self.local_worker_id
    }

    async fn send(&self, dst: EndpointId, message: OutgoingMessage) -> RS<MessageId> {
        scoped_task_trace!();
        let msg_id = self.next_msg_id.fetch_add(1, Ordering::Relaxed);
        let envelope = Envelope::new(
            msg_id,
            message.correlation_id(),
            self.local_endpoint(),
            dst,
            message.kind(),
            message.payload_owned(),
            message.delivery(),
        );
        let target_worker = self.route_worker_index(dst)?;
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
        Ok(msg_id)
    }

    async fn recv(&self, filter: RecvFilter) -> RS<Envelope> {
        let receiver = {
            let mut state = self.state.lock().map_err(|_| {
                mudu_error!(ErrorCode::Internal, "tokio message bus state lock poisoned")
            })?;
            if let Some(envelope) = state.try_take_message(&filter) {
                return Ok(envelope);
            }
            state.register_waiter(filter)
        };
        receiver.wait().await?.ok_or_else(|| {
            mudu_error!(
                ErrorCode::ChannelClosed,
                "message bus waiter dropped before delivery"
            )
        })
    }

    fn on_recv_callback(&self, filter: RecvFilter, callback: OnRecvCallback) -> RS<SubscriptionId> {
        let (callback_id, maybe_envelope) = {
            let mut state = self.state.lock().map_err(|_| {
                mudu_error!(ErrorCode::Internal, "tokio message bus state lock poisoned")
            })?;
            state.register_callback(filter, callback.clone())
        };
        if let Some(envelope) = maybe_envelope {
            let _ = spawn_local_detached("tokio_message_bus_on_recv_callback", async move {
                let _ = (callback)(envelope).await;
            });
        }
        Ok(callback_id)
    }

    fn cancel_callback(&self, id: SubscriptionId) -> RS<bool> {
        let mut state = self.state.lock().map_err(|_| {
            mudu_error!(ErrorCode::Internal, "tokio message bus state lock poisoned")
        })?;
        Ok(state.cancel_callback(id))
    }
}

unsafe impl Send for TokioWorkerMessageBus {}
unsafe impl Sync for TokioWorkerMessageBus {}
