use mudu::channel::{DeliveryMode, Envelope, MessageKind, RecvFilter};
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_sys::sync::async_::notify_wait::{create_notify_wait, Notify, Wait};
use mudu_utils::channel::{channel_now_ms, KindHandler};
use std::collections::{HashMap, VecDeque};

/// Upper bound of buffered responses that arrived before their waiter; the
/// oldest is dropped past it.
const MAX_BUFFERED_RESPONSES: usize = 1024;

struct ChannelRecvWaiter {
    filter: RecvFilter,
    deadline_ms: Option<u64>,
    sender: Notify<Envelope>,
}

/// What the channel implementation must do with an incoming envelope after
/// the state machine consumed it.
pub(crate) enum IncomingDispatch {
    /// Nothing further: a waiter was notified, or the message was dropped
    /// (expired, or fire-and-forget of an unknown kind).
    Delivered,
    /// Spawn the handler with the envelope.
    Dispatch(KindHandler, Envelope),
    /// A request arrived for a kind with no handler: answer it with a nack
    /// so the asker fails fast instead of timing out.
    NackUnknown(Envelope),
    /// A response with no matching waiter was buffered for a later recv.
    Buffered,
}

/// Shared reception state of a worker channel: buffered responses, pending
/// recv waiters, and the kind dispatch table.
pub(crate) struct WorkerChannelState {
    inbox: VecDeque<Envelope>,
    recv_waiters: VecDeque<ChannelRecvWaiter>,
    handlers: HashMap<MessageKind, KindHandler>,
    closed: bool,
}

impl WorkerChannelState {
    pub fn new() -> Self {
        Self {
            inbox: VecDeque::new(),
            recv_waiters: VecDeque::new(),
            handlers: HashMap::new(),
            closed: false,
        }
    }

    pub(crate) fn try_take_message(&mut self, filter: &RecvFilter) -> Option<Envelope> {
        self.evict_expired_inbox();
        let index = self
            .inbox
            .iter()
            .position(|message| message.matches(filter))?;
        self.inbox.remove(index)
    }

    pub(crate) fn register_waiter(
        &mut self,
        filter: RecvFilter,
        deadline_ms: Option<u64>,
    ) -> Wait<Envelope> {
        self.sweep_expired_waiters();
        let (sender, receiver) = create_notify_wait();
        if self.closed {
            // Receiving after close fails fast instead of parking forever:
            // the dropped sender resolves the waiter with a closed channel.
            drop(sender);
            return receiver;
        }
        self.recv_waiters.push_back(ChannelRecvWaiter {
            filter,
            deadline_ms,
            sender,
        });
        receiver
    }

    pub(crate) fn register_kind_handler(
        &mut self,
        kind: MessageKind,
        handler: KindHandler,
    ) -> RS<()> {
        if self.handlers.contains_key(&kind) {
            return Err(mudu_error!(
                ErrorCode::ObjectAlreadyExists,
                format!("worker channel kind {:?} already has a handler", kind)
            ));
        }
        self.handlers.insert(kind, handler);
        Ok(())
    }

    pub(crate) fn unregister_kind_handler(&mut self, kind: MessageKind) -> RS<()> {
        self.handlers.remove(&kind);
        Ok(())
    }

    pub(crate) fn handle_incoming(&mut self, envelope: Envelope) -> IncomingDispatch {
        if self.closed {
            return IncomingDispatch::Delivered;
        }
        if envelope.is_expired_at(channel_now_ms()) {
            return IncomingDispatch::Delivered;
        }
        match envelope.delivery() {
            DeliveryMode::Response => {
                self.sweep_expired_waiters();
                if let Some(index) = self
                    .recv_waiters
                    .iter()
                    .position(|waiter| envelope.matches(&waiter.filter))
                {
                    let waiter = self.recv_waiters.remove(index).expect("waiter at index");
                    let _ = waiter.sender.notify(envelope);
                    return IncomingDispatch::Delivered;
                }
                self.evict_expired_inbox();
                if self.inbox.len() >= MAX_BUFFERED_RESPONSES {
                    self.inbox.pop_front();
                }
                self.inbox.push_back(envelope);
                IncomingDispatch::Buffered
            }
            DeliveryMode::Request | DeliveryMode::FireAndForget => {
                if let Some(handler) = self.handlers.get(&envelope.kind()) {
                    return IncomingDispatch::Dispatch(handler.clone(), envelope);
                }
                if envelope.delivery() == DeliveryMode::Request {
                    IncomingDispatch::NackUnknown(envelope)
                } else {
                    IncomingDispatch::Delivered
                }
            }
        }
    }

    /// Drops buffered responses past their deadline.
    fn evict_expired_inbox(&mut self) {
        let now = channel_now_ms();
        self.inbox.retain(|envelope| !envelope.is_expired_at(now));
    }

    /// Drops waiters past their deadline; their receivers observe a closed
    /// channel, which their (already timed-out) callers never wait on again.
    fn sweep_expired_waiters(&mut self) {
        let now = channel_now_ms();
        self.recv_waiters
            .retain(|waiter| waiter.deadline_ms.is_none_or(|deadline| now < deadline));
    }

    /// Closes the channel state: drops buffered responses and handlers and
    /// fails every pending waiter — dropping its `Notify` sender makes the
    /// receiver observe a closed channel (`ChannelClosed`). Later receives
    /// fail fast instead of parking, and incoming messages are dropped.
    pub(crate) fn close(&mut self) {
        self.closed = true;
        self.inbox.clear();
        self.recv_waiters.clear();
        self.handlers.clear();
    }

    /// Number of pending recv waiters (test introspection).
    #[cfg(test)]
    pub(crate) fn waiter_count(&self) -> usize {
        self.recv_waiters.len()
    }
}
#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use mudu::channel::ReplyStatus;

    const KIND: MessageKind = MessageKind::User(0x7F11);

    fn response(request_id: u128, src: u128, dst: u128) -> Envelope {
        Envelope::new(
            1,
            Some(request_id),
            src,
            dst,
            KIND,
            vec![],
            DeliveryMode::Response,
        )
        .with_request_id(request_id)
    }

    fn request(request_id: u128, kind: MessageKind) -> Envelope {
        Envelope::new(1, None, 2, 1, kind, vec![], DeliveryMode::Request)
            .with_request_id(request_id)
    }

    fn handler() -> KindHandler {
        std::sync::Arc::new(|_envelope| Box::pin(async { Ok(()) }))
    }

    #[test]
    fn response_notifies_matching_waiter() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async {
            let mut state = WorkerChannelState::new();
            let filter = RecvFilter {
                correlation_id: Some(7),
                delivery: Some(DeliveryMode::Response),
                ..RecvFilter::default()
            };
            let waiter = state.register_waiter(filter, None);
            let dispatch = state.handle_incoming(response(7, 2, 1));
            assert!(matches!(dispatch, IncomingDispatch::Delivered));
            let envelope = waiter.wait().await.unwrap().unwrap();
            assert_eq!(envelope.request_id(), 7);
        })
        .unwrap()
    }

    #[test]
    fn response_without_waiter_is_buffered_then_taken() {
        let mut state = WorkerChannelState::new();
        let dispatch = state.handle_incoming(response(9, 2, 1));
        assert!(matches!(dispatch, IncomingDispatch::Buffered));
        let filter = RecvFilter {
            correlation_id: Some(9),
            ..RecvFilter::default()
        };
        let envelope = state.try_take_message(&filter).unwrap();
        assert_eq!(envelope.request_id(), 9);
        // A request is never buffered in the response inbox.
        assert!(state.try_take_message(&RecvFilter::default()).is_none());
    }

    #[test]
    fn expired_response_is_dropped_not_buffered() {
        let mut state = WorkerChannelState::new();
        let expired = response(9, 2, 1).with_deadline_ms(1);
        let dispatch = state.handle_incoming(expired);
        assert!(matches!(dispatch, IncomingDispatch::Delivered));
        assert!(state.try_take_message(&RecvFilter::default()).is_none());
    }

    #[test]
    fn buffered_responses_evict_expired_on_access() {
        let mut state = WorkerChannelState::new();
        // One already-expired entry plus a live one.
        state.handle_incoming(response(1, 2, 1));
        let expired = response(2, 2, 1).with_deadline_ms(channel_now_ms());
        state.inbox.push_back(expired);
        let filter = RecvFilter {
            correlation_id: Some(1),
            ..RecvFilter::default()
        };
        assert!(state.try_take_message(&filter).is_some());
        // The expired entry was evicted during the access.
        assert!(state.inbox.is_empty());
    }

    #[test]
    fn inbox_cap_drops_oldest() {
        let mut state = WorkerChannelState::new();
        for request_id in 0..MAX_BUFFERED_RESPONSES as u128 {
            state.handle_incoming(response(request_id, 2, 1));
        }
        state.handle_incoming(response(MAX_BUFFERED_RESPONSES as u128, 2, 1));
        assert_eq!(state.inbox.len(), MAX_BUFFERED_RESPONSES);
        // The oldest (request_id 0) was evicted.
        let filter = RecvFilter {
            correlation_id: Some(0),
            ..RecvFilter::default()
        };
        assert!(state.try_take_message(&filter).is_none());
    }

    #[test]
    fn request_dispatches_to_kind_handler() {
        let mut state = WorkerChannelState::new();
        state.register_kind_handler(KIND, handler()).unwrap();
        let dispatch = state.handle_incoming(request(1, KIND));
        assert!(matches!(dispatch, IncomingDispatch::Dispatch(_, _)));
    }

    #[test]
    fn unknown_request_nacks_unknown_fire_and_forget_drops() {
        let mut state = WorkerChannelState::new();
        let dispatch = state.handle_incoming(request(1, KIND));
        assert!(matches!(dispatch, IncomingDispatch::NackUnknown(_)));
        let fire_and_forget =
            Envelope::new(1, None, 2, 1, KIND, vec![], DeliveryMode::FireAndForget);
        let dispatch = state.handle_incoming(fire_and_forget);
        assert!(matches!(dispatch, IncomingDispatch::Delivered));
    }

    #[test]
    fn duplicate_kind_registration_fails() {
        let mut state = WorkerChannelState::new();
        state.register_kind_handler(KIND, handler()).unwrap();
        let result = state.register_kind_handler(KIND, handler());
        assert_eq!(result.unwrap_err().ec(), ErrorCode::ObjectAlreadyExists);
        state.unregister_kind_handler(KIND).unwrap();
        state.register_kind_handler(KIND, handler()).unwrap();
    }

    #[test]
    fn expired_waiter_is_swept() {
        let mut state = WorkerChannelState::new();
        // Deadline already passed (1 ms since epoch).
        let waiter = state.register_waiter(RecvFilter::default(), Some(1));
        assert_eq!(state.recv_waiters.len(), 1);
        // The next registration sweeps the expired waiter.
        state.register_waiter(RecvFilter::default(), None);
        assert_eq!(state.recv_waiters.len(), 1);
        drop(waiter);
    }

    #[test]
    fn nack_status_survives_buffering() {
        let mut state = WorkerChannelState::new();
        let nack = response(5, 2, 1).with_status(ReplyStatus::Nack);
        state.handle_incoming(nack);
        let filter = RecvFilter {
            correlation_id: Some(5),
            ..RecvFilter::default()
        };
        let envelope = state.try_take_message(&filter).unwrap();
        assert_eq!(envelope.status(), ReplyStatus::Nack);
    }

    #[test]
    fn close_clears_state_and_fails_waiters() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async {
            let mut state = WorkerChannelState::new();
            state.handle_incoming(response(9, 2, 1));
            state.register_kind_handler(KIND, handler()).unwrap();
            let waiter = state.register_waiter(RecvFilter::default(), None);
            state.close();
            assert!(state.inbox.is_empty());
            assert!(state.recv_waiters.is_empty());
            assert!(state.handlers.is_empty());
            // The dropped sender resolves the waiter with a closed channel.
            assert!(waiter.wait().await.unwrap().is_none());
            // Receiving after close fails fast instead of parking forever.
            let late_waiter = state.register_waiter(RecvFilter::default(), None);
            assert!(late_waiter.wait().await.unwrap().is_none());
            // Incoming messages after close are dropped.
            let dispatch = state.handle_incoming(response(10, 2, 1));
            assert!(matches!(dispatch, IncomingDispatch::Delivered));
            assert!(state.inbox.is_empty());
        })
        .unwrap()
    }
}
