use crate::server::message_bus_api::{Envelope, OnRecvCallback, RecvFilter, SubscriptionId};
use mudu_sys::sync::notify_wait::{create_notify_wait, Notify, Wait};
use std::collections::VecDeque;

struct MessageBusRecvWaiter {
    filter: RecvFilter,
    sender: Notify<Envelope>,
}

struct RegisteredMessageCallback {
    id: SubscriptionId,
    filter: RecvFilter,
    callback: OnRecvCallback,
}

#[derive(Default)]
pub(in crate::server) struct WorkerMessageBusState {
    inbox: VecDeque<Envelope>,
    recv_waiters: VecDeque<MessageBusRecvWaiter>,
    callbacks: Vec<RegisteredMessageCallback>,
    next_subscription_id: SubscriptionId,
}

impl WorkerMessageBusState {
    pub(in crate::server) fn new() -> Self {
        Self {
            next_subscription_id: 1,
            ..Self::default()
        }
    }

    pub(in crate::server) fn try_take_message(&mut self, filter: &RecvFilter) -> Option<Envelope> {
        let index = self
            .inbox
            .iter()
            .position(|message| message.matches(filter))?;
        self.inbox.remove(index)
    }

    pub(in crate::server) fn register_waiter(&mut self, filter: RecvFilter) -> Wait<Envelope> {
        let (sender, receiver) = create_notify_wait();
        self.recv_waiters
            .push_back(MessageBusRecvWaiter { filter, sender });
        receiver
    }

    pub(in crate::server) fn register_callback(
        &mut self,
        filter: RecvFilter,
        callback: OnRecvCallback,
    ) -> (SubscriptionId, Option<Envelope>) {
        let id = self.next_subscription_id;
        self.next_subscription_id += 1;
        let maybe_envelope = self.try_take_message(&filter);
        self.callbacks.push(RegisteredMessageCallback {
            id,
            filter,
            callback,
        });
        (id, maybe_envelope)
    }

    pub(in crate::server) fn cancel_callback(&mut self, id: SubscriptionId) -> bool {
        let Some(index) = self.callbacks.iter().position(|callback| callback.id == id) else {
            return false;
        };
        self.callbacks.remove(index);
        true
    }

    pub(in crate::server) fn handle_incoming(
        &mut self,
        envelope: Envelope,
    ) -> Option<(OnRecvCallback, Envelope)> {
        if let Some(index) = self
            .recv_waiters
            .iter()
            .position(|waiter| envelope.matches(&waiter.filter))
        {
            if let Some(waiter) = self.recv_waiters.remove(index) {
                let _ = waiter.sender.notify(envelope);
                return None;
            }
        }

        if let Some(index) = self
            .callbacks
            .iter()
            .position(|callback| envelope.matches(&callback.filter))
        {
            let callback = self.callbacks[index].callback.clone();
            return Some((callback, envelope));
        }

        self.inbox.push_back(envelope);
        None
    }
}
