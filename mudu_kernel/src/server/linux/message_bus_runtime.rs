use async_trait::async_trait;
use crossbeam_queue::SegQueue;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use std::os::fd::RawFd;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use mudu_sys::sync::SMutex;

use crate::server::message_bus_api::{
    EndpointId, Envelope, MessageBus, MessageBusRef, MessageId, OnRecvCallback, OutgoingMessage,
    RecvFilter, SubscriptionId,
};
use crate::server::message_bus_state::WorkerMessageBusState;
use crate::server::server_iouring;
use crate::server::task;
use crate::server::worker_mailbox::WorkerMailboxMsg;
use crate::server::worker_registry::WorkerRegistry;
use mudu_sys::server::worker_task::spawn_system_worker_task;
use tracing::debug;

pub(crate) struct WorkerMessageBus {
    local_worker_id: OID,
    registry: Arc<WorkerRegistry>,
    mailbox_fds: Vec<RawFd>,
    mailboxes: Vec<Arc<SegQueue<WorkerMailboxMsg>>>,
    next_msg_id: AtomicU64,
    state: SMutex<WorkerMessageBusState>,
}

unsafe impl Send for WorkerMessageBus {}
unsafe impl Sync for WorkerMessageBus {}

impl WorkerMessageBus {
    pub(crate) fn new(
        local_worker_id: OID,
        registry: Arc<WorkerRegistry>,
        mailbox_fds: Vec<RawFd>,
        mailboxes: Vec<Arc<SegQueue<WorkerMailboxMsg>>>,
    ) -> Arc<Self> {
        Arc::new(Self {
            local_worker_id,
            registry,
            mailbox_fds,
            mailboxes,
            next_msg_id: AtomicU64::new(1),
            state: SMutex::new(WorkerMessageBusState::new()),
        })
    }

    pub(crate) fn as_ref(self: &Arc<Self>) -> MessageBusRef {
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
            "message_bus handle incoming"
        );
        let maybe_callback = {
            let mut state = self
                .state
                .lock()
                .map_err(|_| m_error!(EC::InternalErr, "message bus state lock poisoned"))?;
            state.handle_incoming(envelope)
        };
        if let Some((callback, envelope)) = maybe_callback {
            debug!(
                local_worker_id = self.local_worker_id,
                src = ?envelope.src(),
                dst = ?envelope.dst(),
                kind = ?envelope.kind(),
                msg_id = envelope.msg_id(),
                "message_bus dispatching callback task"
            );
            let future = (callback)(envelope);
            task::spawn_system(
                "iouring-message-bus-callback",
                spawn_system_worker_task(future),
            );
        }
        Ok(())
    }

    fn route_worker_index(&self, endpoint: EndpointId) -> RS<usize> {
        self.registry
            .worker_index_by_worker_id(endpoint)
            .ok_or_else(|| m_error!(EC::NoSuchElement, format!("no such worker id {}", endpoint)))
    }

    fn dispatch_mailbox_message(&self, target_worker: usize, msg: WorkerMailboxMsg) -> RS<()> {
        let Some(mailbox) = self.mailboxes.get(target_worker) else {
            return Err(m_error!(
                EC::InternalErr,
                format!("mailbox target worker {} is out of range", target_worker)
            ));
        };
        let Some(&fd) = self.mailbox_fds.get(target_worker) else {
            return Err(m_error!(
                EC::InternalErr,
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
            "message_bus enqueue mailbox message"
        );
        mailbox.push(msg);
        server_iouring::notify_mailbox_fd(fd)
    }
}

#[async_trait]
impl MessageBus for WorkerMessageBus {
    fn local_endpoint(&self) -> EndpointId {
        self.local_worker_id
    }

    async fn send(&self, dst: EndpointId, message: OutgoingMessage) -> RS<MessageId> {
        mudu_utils::scoped_task_trace!();
        let msg_id = self.next_msg_id.fetch_add(1, Ordering::Relaxed);
        let envelope = Envelope::new(
            msg_id,
            message.correlation_id(),
            self.local_endpoint(),
            dst.clone(),
            message.kind(),
            message.payload_owned(),
            message.delivery(),
        );
        let target_worker = self.route_worker_index(dst)?;
        debug!(
            local_worker_id = self.local_worker_id,
            dst = ?dst,
            target_worker,
            kind = ?envelope.kind(),
            msg_id,
            correlation_id = ?envelope.correlation_id(),
            "message_bus send"
        );
        self.dispatch_mailbox_message(target_worker, WorkerMailboxMsg::BusMessage(envelope))?;
        Ok(msg_id)
    }

    async fn recv(&self, filter: RecvFilter) -> RS<Envelope> {
        mudu_utils::scoped_task_trace!();
        let receiver = {
            let mut state = self
                .state
                .lock()
                .map_err(|_| m_error!(EC::InternalErr, "message bus state lock poisoned"))?;
            if let Some(envelope) = state.try_take_message(&filter) {
                return Ok(envelope);
            }
            state.register_waiter(filter)
        };
        receiver
            .wait()
            .await?
            .ok_or_else(|| m_error!(EC::ThreadErr, "message bus waiter dropped before delivery"))
    }

    fn on_recv_callback(&self, filter: RecvFilter, callback: OnRecvCallback) -> RS<SubscriptionId> {
        let (callback_id, maybe_envelope) = {
            let mut state = self
                .state
                .lock()
                .map_err(|_| m_error!(EC::InternalErr, "message bus state lock poisoned"))?;
            state.register_callback(filter, callback.clone())
        };
        if let Some(envelope) = maybe_envelope {
            let future = (callback)(envelope);
            task::spawn_system(
                "iouring-message-bus-on-recv",
                spawn_system_worker_task(future),
            );
        }
        Ok(callback_id)
    }

    fn cancel_callback(&self, id: SubscriptionId) -> RS<bool> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| m_error!(EC::InternalErr, "message bus state lock poisoned"))?;
        Ok(state.cancel_callback(id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::message_bus_api::{DeliveryMode, MessageKind, SystemMessageKind};
    use crate::server::worker_registry::WorkerRegistry;

    fn test_registry() -> Arc<WorkerRegistry> {
        Arc::new(
            WorkerRegistry::new(vec![
                crate::server::worker_registry::WorkerIdentity {
                    worker_index: 0,
                    worker_id: 11,
                    partition_ids: vec![101],
                },
                crate::server::worker_registry::WorkerIdentity {
                    worker_index: 1,
                    worker_id: 12,
                    partition_ids: vec![102],
                },
            ])
            .unwrap(),
        )
    }

    fn test_bus(worker_id: OID) -> Arc<WorkerMessageBus> {
        WorkerMessageBus::new(
            worker_id,
            test_registry(),
            vec![0, 1],
            vec![Arc::new(SegQueue::new()), Arc::new(SegQueue::new())],
        )
    }

    #[tokio::test]
    async fn recv_consumes_buffered_message() {
        let bus = test_bus(11);
        bus.handle_incoming(Envelope::new(
            1,
            None,
            12,
            11,
            MessageKind::User(7),
            b"ping".to_vec(),
            DeliveryMode::FireAndForget,
        ))
        .unwrap();

        let message = bus
            .recv(RecvFilter {
                src: Some(12),
                kind: Some(MessageKind::User(7)),
                ..RecvFilter::default()
            })
            .await
            .unwrap();
        assert_eq!(message.payload(), b"ping");
    }

    #[tokio::test]
    async fn recv_waiter_is_fulfilled_by_incoming_message() {
        let bus = test_bus(11);
        let mut recv = Box::pin(bus.recv(RecvFilter {
            src: Some(12),
            correlation_id: Some(9),
            ..RecvFilter::default()
        }));

        assert!(matches!(
            futures::poll!(recv.as_mut()),
            std::task::Poll::Pending
        ));

        bus.handle_incoming(Envelope::new(
            2,
            Some(9),
            12,
            11,
            MessageKind::System(SystemMessageKind::Ack),
            Vec::new(),
            DeliveryMode::Response,
        ))
        .unwrap();

        let message = recv.await.unwrap();
        assert_eq!(message.correlation_id(), Some(9));
    }
}
