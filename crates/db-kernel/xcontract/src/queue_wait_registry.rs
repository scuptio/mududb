//! Per-worker registry of parked queue `RECEIVE ... WAIT` waiters, keyed by
//! queue shard.
//!
//! Modeled on the lock manager's waiter design (`x_lock_mgr.rs`): every queue
//! shard with parked receivers owns a FIFO queue of waiters, each waiter parks
//! on its own private [`mudu_sys::sync::async_::ANotify`], and the shard's commit apply path delivers
//! a targeted wakeup to the head waiter only (P0 semantics: a single active
//! consumer per shard, so there is no fan-out and no thundering herd).
//!
//! Lost-wakeup freedom comes from the registration order, not from holding a
//! lock across the queue scan: the executor registers its waiter *before* the
//! scan whose result justifies parking and clears the latch's sticky signal
//! before every scan round. A commit notification therefore either applies
//! before that scan (the scan observes the new messages) or lands after the
//! clear (the sticky signal re-arms and the park returns at once).

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::AtomicU64;

use mudu_sys::sync::async_::ANotify;
use mudu_sys::task::async_::timeout;

use ::xengine::api::QueueWaiter;
use ::xengine::tx_mgr::TxMgr;
use async_trait::async_trait;

use crate::queue_activation::ActivationRegistry;
use crate::queue_types::QueueShardId;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu_sys::sync::SMutex;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

/// One parked waiter: a unique sequence number (identity for targeted
/// removal) plus the private wake latch only this waiter ever parks on.
struct WaiterEntry {
    seq: u64,
    shard: QueueShardId,
    notify: ANotify,
}

#[derive(Default)]
struct WaitRegistryState {
    shards: HashMap<QueueShardId, VecDeque<Arc<WaiterEntry>>>,
    next_waiter_seq: u64,
}

/// Per-worker registry of parked queue `RECEIVE ... WAIT` waiters, keyed by
/// queue shard (`QueueShardId`).
pub struct QueueWaitRegistry {
    state: Arc<SMutex<WaitRegistryState>>,
    /// Live waiter count across all shards; the commit hook consults it to
    /// skip the write-set scan entirely while nobody waits.
    waiter_count: Arc<AtomicU64>,
}

/// RAII handle of one registered waiter; implements [`QueueWaiter`]. Dropping
/// it unregisters the waiter, so timed-out, finished, or cancelled statements
/// never leak their registration.
pub struct QueueWaiterGuard {
    state: Arc<SMutex<WaitRegistryState>>,
    waiter_count: Arc<AtomicU64>,
    entry: Arc<WaiterEntry>,
}

impl QueueWaitRegistry {
    /// Creates an empty registry.
    pub fn new() -> Self {
        Self {
            state: Arc::new(SMutex::new(WaitRegistryState::default())),
            waiter_count: Arc::new(AtomicU64::new(0)),
        }
    }

    fn lock(&self) -> RS<mudu_sys::sync::SMutexGuard<'_, WaitRegistryState>> {
        self.state.lock()
    }

    /// Registers a waiter at the tail of the shard's FIFO queue and returns
    /// its RAII park latch.
    pub fn register(&self, shard: QueueShardId) -> RS<Arc<QueueWaiterGuard>> {
        let entry = {
            let mut state = self.lock()?;
            let seq = state.next_waiter_seq;
            state.next_waiter_seq += 1;
            let entry = Arc::new(WaiterEntry {
                seq,
                shard,
                notify: ANotify::new(),
            });
            state
                .shards
                .entry(shard)
                .or_default()
                .push_back(entry.clone());
            entry
        };
        self.waiter_count.fetch_add(1, Ordering::Relaxed);
        Ok(Arc::new(QueueWaiterGuard {
            state: self.state.clone(),
            waiter_count: self.waiter_count.clone(),
            entry,
        }))
    }

    /// Wakes the head waiter of `shard`, if any. Cheap when nothing waits:
    /// one atomic load short-circuits before the map is touched.
    pub fn notify_one(&self, shard: QueueShardId) -> RS<()> {
        if self.waiter_count.load(Ordering::Relaxed) == 0 {
            return Ok(());
        }
        let head = {
            let state = self.lock()?;
            state
                .shards
                .get(&shard)
                .and_then(|queue| queue.front().cloned())
        };
        // ANotify wakeups are sticky: the woken waiter observes the signal
        // even when it has not parked yet.
        if let Some(head) = head {
            head.notify.notify_waiters();
        }
        Ok(())
    }

    /// Wakes every waiter of `shard` without removing any registration
    /// (unlike [`QueueWaitRegistry::remove_queue`]). Used when the queue's
    /// consumer set changed (a dropped cursor must make its parked
    /// `RECEIVE ... FOR` re-scan and fail fast) while the queue itself lives
    /// on and the other cursors' waiters keep their registrations.
    pub fn notify_all(&self, shard: QueueShardId) -> RS<()> {
        if self.waiter_count.load(Ordering::Relaxed) == 0 {
            return Ok(());
        }
        let waiters: Vec<Arc<WaiterEntry>> = {
            let state = self.lock()?;
            state
                .shards
                .get(&shard)
                .map(|queue| queue.iter().cloned().collect())
                .unwrap_or_default()
        };
        for waiter in waiters {
            waiter.notify.notify_waiters();
        }
        Ok(())
    }

    /// Whether any waiter is parked on any shard; the commit hook uses this
    /// to skip the write-set scan while nobody waits.
    pub fn has_waiters(&self) -> bool {
        self.waiter_count.load(Ordering::Relaxed) > 0
    }

    /// Per-shard parked-waiter counts, for the observability endpoint.
    pub fn waiter_counts(&self) -> RS<HashMap<QueueShardId, u64>> {
        Ok(self
            .lock()?
            .shards
            .iter()
            .map(|(shard, queue)| (*shard, queue.len() as u64))
            .collect())
    }

    /// Removes every waiter of every shard of the queue `queue_oid`, waking
    /// them so parked `WAIT FOREVER` statements re-scan (and fail on the
    /// dropped queue) instead of hanging. Called when the queue is dropped.
    pub fn remove_queue(&self, queue_oid: OID) -> RS<()> {
        let waiters: Vec<Arc<WaiterEntry>> = {
            let mut state = self.lock()?;
            let shards: Vec<QueueShardId> = state
                .shards
                .keys()
                .filter(|(oid, _)| *oid == queue_oid)
                .copied()
                .collect();
            shards
                .into_iter()
                .flat_map(|shard| state.shards.remove(&shard).unwrap_or_default())
                .collect()
        };
        for waiter in waiters {
            waiter.notify.notify_waiters();
        }
        Ok(())
    }

    /// Wakes and removes every waiter of every shard (storage reload
    /// underneath, e.g. a snapshot restore), so parked waiters re-scan
    /// against the reloaded relations instead of hanging.
    pub fn clear(&self) -> RS<()> {
        let waiters: Vec<Arc<WaiterEntry>> = {
            let mut state = self.lock()?;
            state.shards.drain().flat_map(|(_, queue)| queue).collect()
        };
        for waiter in waiters {
            waiter.notify.notify_waiters();
        }
        Ok(())
    }
}

impl Default for QueueWaitRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl QueueWaiter for QueueWaiterGuard {
    fn clear_signal(&self) {
        self.entry.notify.clear_signal();
    }

    async fn park(&self, timeout_duration: Option<Duration>) {
        match timeout_duration {
            Some(duration) => {
                let _ = timeout(duration, self.entry.notify.notified()).await;
            }
            None => {
                self.entry.notify.notified().await;
            }
        }
    }
}

impl Drop for QueueWaiterGuard {
    fn drop(&mut self) {
        if let Ok(mut state) = self.state.lock() {
            let mut became_empty = false;
            if let Some(queue) = state.shards.get_mut(&self.entry.shard) {
                queue.retain(|waiter| waiter.seq != self.entry.seq);
                became_empty = queue.is_empty();
            }
            if became_empty {
                state.shards.remove(&self.entry.shard);
            }
        }
        // `remove_queue`/`clear` may have taken the entry out already; every
        // guard decrements exactly once, no matter which side removed it.
        self.waiter_count.fetch_sub(1, Ordering::Relaxed);
    }
}

/// Distinct queue shards of the transaction's write set, collected only
/// when some queue receiver is parked or some queue activation is
/// registered; empty otherwise, so a commit without any queue consumer of
/// either kind pays one atomic load and nothing else.
pub(crate) fn queue_wait_wake_tables(
    queue_wait_registry: &QueueWaitRegistry,
    activation_registry: &ActivationRegistry,
    tx: &dyn TxMgr,
) -> Vec<QueueShardId> {
    if !queue_wait_registry.has_waiters() && !activation_registry.has_any() {
        return Vec::new();
    }
    let mut shards: Vec<QueueShardId> = tx
        .write_ops()
        .iter()
        .map(|(relation_id, _)| (relation_id.table_id, relation_id.partition_id))
        .collect();
    shards.sort_unstable();
    shards.dedup();
    shards
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn notify_one_wakes_the_registered_waiter() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let registry = QueueWaitRegistry::new();
            let waiter = registry.register((7, 0)).unwrap();
            waiter.clear_signal();
            registry.notify_one((7, 0)).unwrap();
            // The sticky signal makes the park return immediately.
            timeout(Duration::from_secs(5), waiter.park(None))
                .await
                .expect("the notified waiter must wake");
        })
        .unwrap()
    }

    #[test]
    fn park_returns_on_timeout_without_a_notification() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let registry = QueueWaitRegistry::new();
            let waiter = registry.register((7, 0)).unwrap();
            waiter.clear_signal();
            let started = mudu_sys::time::instant_now();
            waiter.park(Some(Duration::from_millis(30))).await;
            assert!(started.elapsed() >= Duration::from_millis(20));
        })
        .unwrap()
    }

    #[test]
    fn notify_one_targets_the_fifo_head_only() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let registry = QueueWaitRegistry::new();
            let head = registry.register((7, 0)).unwrap();
            let second = registry.register((7, 0)).unwrap();
            head.clear_signal();
            second.clear_signal();
            registry.notify_one((7, 0)).unwrap();
            // The head observes the wakeup; the second waiter does not and
            // its park runs into the timeout.
            head.park(Some(Duration::from_secs(5))).await;
            let started = mudu_sys::time::instant_now();
            second.park(Some(Duration::from_millis(30))).await;
            assert!(started.elapsed() >= Duration::from_millis(20));
            // With the head gone, the next notify reaches the second waiter.
            drop(head);
            second.clear_signal();
            registry.notify_one((7, 0)).unwrap();
            second.park(Some(Duration::from_secs(5))).await;
        })
        .unwrap()
    }

    #[test]
    fn drop_unregisters_the_waiter() {
        let registry = QueueWaitRegistry::new();
        let waiter = registry.register((7, 0)).unwrap();
        assert!(registry.has_waiters());
        assert_eq!(registry.waiter_counts().unwrap().get(&(7, 0)), Some(&1));
        drop(waiter);
        assert!(!registry.has_waiters());
        assert!(registry.waiter_counts().unwrap().is_empty());
    }

    #[test]
    fn notify_all_wakes_every_waiter_and_keeps_registrations() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let registry = QueueWaitRegistry::new();
            let first = registry.register((7, 0)).unwrap();
            let second = registry.register((7, 0)).unwrap();
            first.clear_signal();
            second.clear_signal();
            registry.notify_all((7, 0)).unwrap();
            // Both waiters observe the wake (unlike notify_one, which only
            // reaches the FIFO head).
            first.park(Some(Duration::from_secs(5))).await;
            second.park(Some(Duration::from_secs(5))).await;
            // The registrations stay: both waiters can be woken again.
            assert_eq!(registry.waiter_counts().unwrap().get(&(7, 0)), Some(&2));
            first.clear_signal();
            second.clear_signal();
            registry.notify_all((7, 0)).unwrap();
            first.park(Some(Duration::from_secs(5))).await;
            second.park(Some(Duration::from_secs(5))).await;
        })
        .unwrap()
    }

    #[test]
    fn notify_without_waiters_is_a_no_op() {
        let registry = QueueWaitRegistry::new();
        assert!(!registry.has_waiters());
        registry.notify_one((7, 0)).unwrap();
    }

    #[test]
    fn remove_queue_wakes_all_shards_of_the_queue() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let registry = QueueWaitRegistry::new();
            let first = registry.register((7, 0)).unwrap();
            let second = registry.register((7, 9)).unwrap();
            let other = registry.register((8, 0)).unwrap();
            first.clear_signal();
            second.clear_signal();
            other.clear_signal();
            registry.remove_queue(7).unwrap();
            // Both shard waiters of queue 7 observe the wake; queue 8 is
            // untouched.
            first.park(Some(Duration::from_secs(5))).await;
            second.park(Some(Duration::from_secs(5))).await;
            let counts = registry.waiter_counts().unwrap();
            assert!(!counts.contains_key(&(7, 0)));
            assert!(!counts.contains_key(&(7, 9)));
            assert_eq!(counts.get(&(8, 0)), Some(&1));
            let started = mudu_sys::time::instant_now();
            other.park(Some(Duration::from_millis(30))).await;
            assert!(started.elapsed() >= Duration::from_millis(20));
        })
        .unwrap()
    }

    #[test]
    fn clear_wakes_and_removes_every_waiter() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let registry = QueueWaitRegistry::new();
            let waiter_a = registry.register((7, 0)).unwrap();
            let waiter_b = registry.register((8, 3)).unwrap();
            waiter_a.clear_signal();
            waiter_b.clear_signal();
            registry.clear().unwrap();
            waiter_a.park(Some(Duration::from_secs(5))).await;
            waiter_b.park(Some(Duration::from_secs(5))).await;
            assert!(registry.waiter_counts().unwrap().is_empty());
        })
        .unwrap()
    }
}
