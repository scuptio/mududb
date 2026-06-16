#![allow(dead_code)]

use std::collections::{BTreeMap, HashMap, HashSet};
use std::future::Future;
use std::pin::Pin;

use mudu::common::result::RS;

pub type CallbackId = u64;
pub type CallbackFuture = Pin<Box<dyn Future<Output = RS<()>> + Send>>;
pub type AsyncCallback = Box<dyn FnOnce() -> CallbackFuture + Send>;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct CallbackEventKey {
    pub kind: u16,
    pub id: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub enum CallbackDomain {
    Generic(u16),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CallbackTrigger {
    Event(CallbackEventKey),
    Sequence { domain: CallbackDomain, target: u64 },
}

pub struct PendingCallback {
    pub id: CallbackId,
    pub trigger: CallbackTrigger,
    pub callback: AsyncCallback,
}

pub(in crate::server) struct CallbackRegistry {
    next_id: CallbackId,
    event_waiters: HashMap<CallbackEventKey, Vec<PendingCallback>>,
    sequence_waiters: HashMap<CallbackDomain, BTreeMap<u64, Vec<PendingCallback>>>,
    cancelled: HashSet<CallbackId>,
}

impl CallbackRegistry {
    pub(in crate::server) fn new() -> Self {
        Self {
            next_id: 1,
            event_waiters: HashMap::new(),
            sequence_waiters: HashMap::new(),
            cancelled: HashSet::new(),
        }
    }

    pub(in crate::server) fn register(
        &mut self,
        trigger: CallbackTrigger,
        callback: AsyncCallback,
    ) -> CallbackId {
        let id = self.next_id;
        self.next_id += 1;
        let pending = PendingCallback {
            id,
            trigger,
            callback,
        };
        match trigger {
            CallbackTrigger::Event(key) => {
                self.event_waiters.entry(key).or_default().push(pending);
            }
            CallbackTrigger::Sequence { domain, target } => {
                self.sequence_waiters
                    .entry(domain)
                    .or_default()
                    .entry(target)
                    .or_default()
                    .push(pending);
            }
        }
        id
    }

    pub(in crate::server) fn register_event(
        &mut self,
        key: CallbackEventKey,
        callback: AsyncCallback,
    ) -> CallbackId {
        self.register(CallbackTrigger::Event(key), callback)
    }

    pub(in crate::server) fn register_after(
        &mut self,
        domain: CallbackDomain,
        target: u64,
        callback: AsyncCallback,
    ) -> CallbackId {
        self.register(CallbackTrigger::Sequence { domain, target }, callback)
    }

    pub(in crate::server) fn cancel(&mut self, id: CallbackId) -> bool {
        self.cancelled.insert(id)
    }

    pub(in crate::server) fn fire_event(&mut self, key: CallbackEventKey) -> Vec<PendingCallback> {
        let Some(callbacks) = self.event_waiters.remove(&key) else {
            return Vec::new();
        };
        self.filter_cancelled(callbacks)
    }

    pub(in crate::server) fn advance_sequence(
        &mut self,
        domain: CallbackDomain,
        value: u64,
    ) -> Vec<PendingCallback> {
        let Some(waiters) = self.sequence_waiters.get_mut(&domain) else {
            return Vec::new();
        };

        let targets: Vec<u64> = waiters.range(..=value).map(|(&target, _)| target).collect();
        let mut ready = Vec::new();
        for target in targets {
            if let Some(callbacks) = waiters.remove(&target) {
                ready.extend(callbacks);
            }
        }
        if waiters.is_empty() {
            self.sequence_waiters.remove(&domain);
        }
        self.filter_cancelled(ready)
    }

    pub(in crate::server) fn is_empty(&self) -> bool {
        self.event_waiters.is_empty() && self.sequence_waiters.is_empty()
    }

    fn filter_cancelled(&mut self, callbacks: Vec<PendingCallback>) -> Vec<PendingCallback> {
        callbacks
            .into_iter()
            .filter(|pending| !self.cancelled.remove(&pending.id))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    #[test]
    fn fires_event_callbacks_once() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let mut registry = CallbackRegistry::new();
        let hit = Arc::new(AtomicUsize::new(0));
        let hit_clone = hit.clone();
        registry.register_event(
            CallbackEventKey { kind: 1, id: 7 },
            Box::new(move || {
                Box::pin(async move {
                    hit_clone.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                })
            }),
        );

        let ready = registry.fire_event(CallbackEventKey { kind: 1, id: 7 });
        assert_eq!(ready.len(), 1);
        assert_eq!(
            ready[0].trigger,
            CallbackTrigger::Event(CallbackEventKey { kind: 1, id: 7 })
        );
        for callback in ready {
            (callback.callback)().await.unwrap();
        }
        assert_eq!(hit.load(Ordering::SeqCst), 1);
        assert!(registry
            .fire_event(CallbackEventKey { kind: 1, id: 7 })
            .is_empty());
        assert!(registry.is_empty());
        }).unwrap()
    }

    #[test]
    fn advances_sequence_callbacks_up_to_frontier() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let mut registry = CallbackRegistry::new();
        let hit = Arc::new(AtomicUsize::new(0));
        for target in [3u64, 5u64] {
            let hit_clone = hit.clone();
            registry.register_after(
                CallbackDomain::Generic(9),
                target,
                Box::new(move || {
                    Box::pin(async move {
                        hit_clone.fetch_add(1, Ordering::SeqCst);
                        Ok(())
                    })
                }),
            );
        }

        let ready = registry.advance_sequence(CallbackDomain::Generic(9), 3);
        assert_eq!(ready.len(), 1);
        assert_eq!(
            ready[0].trigger,
            CallbackTrigger::Sequence {
                domain: CallbackDomain::Generic(9),
                target: 3,
            }
        );
        for callback in ready {
            (callback.callback)().await.unwrap();
        }
        assert_eq!(hit.load(Ordering::SeqCst), 1);

        let ready = registry.advance_sequence(CallbackDomain::Generic(9), 5);
        assert_eq!(ready.len(), 1);
        for callback in ready {
            (callback.callback)().await.unwrap();
        }
        assert_eq!(hit.load(Ordering::SeqCst), 2);
        }).unwrap()
    }

    #[test]
    fn cancelled_callback_is_filtered_before_execution() {
        let mut registry = CallbackRegistry::new();
        let id = registry.register_event(
            CallbackEventKey { kind: 2, id: 11 },
            Box::new(|| Box::pin(async { Ok(()) })),
        );
        assert!(registry.cancel(id));
        let ready = registry.fire_event(CallbackEventKey { kind: 2, id: 11 });
        assert!(ready.is_empty());
        assert!(registry.is_empty());
    }
}
