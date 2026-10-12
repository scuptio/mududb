use ::xengine::tx_mgr::PhysicalRelationId;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu_sys::sync::async_::ANotify;
use mudu_sys::sync::SMutex;
use mudu_sys::time::instant_now;
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// A lock table owner: the owning transaction/lock-token and when the lock
/// was taken. The timestamp only backs orphan reclamation: statement-level
/// pessimistic locks survive across RPCs, and a coordinator that dies before
/// commit/rollback would otherwise pin its keys forever.
type LockOwner = (OID, std::time::Instant);

/// Orphan statement-lock time-to-live. Commit-path locks are held for
/// milliseconds, so a 30s TTL only ever fires for leaked statement locks.
const STATEMENT_LOCK_TTL: Duration = Duration::from_secs(30);

/// A parked `lock_some` request. One waiter is registered at the tail of the
/// FIFO queue of every requested key it does not already own, and parks on
/// its own private notify latch, so a targeted `notify_waiters` wakes exactly
/// this waiter instead of every parked transaction (no thundering herd).
struct Waiter {
    /// Unique id identifying this waiter across all per-key queues.
    seq: u64,
    /// Private wakeup latch; only this waiter ever parks on it.
    notify: ANotify,
}

/// Per-key lock state: the current owner (if any) plus the FIFO queue of
/// parked waiters. The queue head has grant priority: when the key is
/// unowned, only the head waiter may take it.
struct KeyLockEntry {
    owner: Option<LockOwner>,
    queue: VecDeque<Arc<Waiter>>,
}

impl KeyLockEntry {
    fn new() -> Self {
        Self {
            owner: None,
            queue: VecDeque::new(),
        }
    }
}

struct LockState {
    tables: HashMap<PhysicalRelationId, HashMap<Vec<u8>, KeyLockEntry>>,
    next_waiter_seq: u64,
}

/// Per-worker lock table for statement-level pessimistic write locks and
/// commit locks, keyed by `(relation, key)`. Acquisition is all-or-nothing
/// with FIFO-fair waiting and a bounded timeout; orphaned locks are reclaimed
/// after a TTL.
pub struct XLockMgr {
    lock: SMutex<LockState>,
    /// Diagnostic counter of targeted wakeups sent; tests use it to verify
    /// that releases wake only the waiters of the released keys.
    wake_count: AtomicU64,
}

impl XLockMgr {
    /// Create an empty lock manager.
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self {
            lock: SMutex::new(LockState {
                tables: HashMap::new(),
                next_waiter_seq: 0,
            }),
            wake_count: AtomicU64::new(0),
        }
    }

    /// Non-waiting acquire: succeeds only when every key is immediately
    /// available. A key with parked waiters counts as unavailable even when
    /// currently unowned — letting `try_lock_some` barge ahead of the FIFO
    /// queue would starve parked waiters, so it fails instead of jumping the
    /// queue (previously it could win such a race against just-woken
    /// waiters).
    pub(crate) fn try_lock_some(
        &self,
        oid: OID,
        table_keys: &[(PhysicalRelationId, Vec<u8>)],
    ) -> RS<bool> {
        mudu_utils::scoped_task_trace!();
        let mut state = self.lock.lock()?;
        let mut wakes = Vec::new();
        let acquired = try_acquire_locked(&mut state, oid, table_keys, None, &mut wakes);
        drop(state);
        self.send_wakes(wakes);
        Ok(acquired)
    }

    /// Acquire the whole key set atomically, waiting (up to `timeout`) while
    /// another transaction holds any of them. Returns `Ok(false)` when the
    /// timeout expires.
    ///
    /// Deadlock-freedom: acquisition stays all-or-nothing (a failed attempt
    /// holds no locks while parked), so no hold-and-wait cycle can form
    /// regardless of key ordering.
    ///
    /// Fairness: on the first failed attempt the waiter is appended to the
    /// FIFO queue of every requested key it does not own and then parks on
    /// its own `ANotify`. A release wakes only the head waiter of each
    /// released key; woken waiters re-attempt and keep their queue positions
    /// across retries, so grants follow arrival order per key instead of a
    /// thundering-herd free-for-all.
    pub(crate) async fn lock_some(
        &self,
        oid: OID,
        table_keys: &[(PhysicalRelationId, Vec<u8>)],
        timeout: Duration,
    ) -> RS<bool> {
        // Fast path: uncontended (or re-entrant) acquire never queues.
        if self.try_lock_some(oid, table_keys)? {
            return Ok(true);
        }
        let waiter = {
            let mut state = self.lock.lock()?;
            let seq = state.next_waiter_seq;
            state.next_waiter_seq += 1;
            Arc::new(Waiter {
                seq,
                notify: ANotify::new(),
            })
        };
        let deadline = instant_now() + timeout;
        loop {
            let mut wakes = Vec::new();
            {
                let mut state = self.lock.lock()?;
                if try_acquire_locked(&mut state, oid, table_keys, Some(&waiter), &mut wakes) {
                    drop(state);
                    self.send_wakes(wakes);
                    return Ok(true);
                }
                enqueue_locked(&mut state, oid, table_keys, &waiter);
                // Clear our sticky signal while still holding the lock: a
                // targeted notify can only come from a lock holder that saw
                // our registration, hence only after this clear, so no wakeup
                // is lost between the failed attempt and parking.
                waiter.notify.clear_signal();
                drop(state);
            }
            self.send_wakes(wakes);
            let now = instant_now();
            if now >= deadline {
                // Timed out: leave every queue; where we were head of a
                // still-unowned key, hand the wakeup to the next waiter so
                // the queue cannot stall behind a dead head.
                let mut state = self.lock.lock()?;
                let wakes = dequeue_locked(&mut state, table_keys, waiter.seq);
                drop(state);
                self.send_wakes(wakes);
                return Ok(false);
            }
            let _ = mudu_sys::task::async_::timeout(deadline - now, waiter.notify.notified()).await;
        }
    }

    pub(crate) fn release(&self, oid: OID, table_keys: &[(PhysicalRelationId, Vec<u8>)]) -> RS<()> {
        let mut state = self.lock.lock()?;
        let mut wakes = Vec::new();
        for (relation_id, key) in table_keys.iter() {
            let mut remove_relation = false;
            if let Some(map) = state.tables.get_mut(relation_id) {
                if let Some(entry) = map.get_mut(key) {
                    if entry.owner.as_ref().is_some_and(|(tx, _)| *tx == oid) {
                        entry.owner = None;
                        if let Some(head) = entry.queue.front() {
                            wakes.push(head.clone());
                        }
                    }
                    if entry.owner.is_none() && entry.queue.is_empty() {
                        map.remove(key);
                    }
                }
                remove_relation = map.is_empty();
            }
            if remove_relation {
                state.tables.remove(relation_id);
            }
        }
        drop(state);
        self.send_wakes(wakes);
        Ok(())
    }

    /// Release every lock held by `oid` across all relations: statement-level
    /// locks whose keys never made it into the write set (e.g. an UPDATE that
    /// matched nothing) are released along with the commit-path locks.
    pub(crate) fn release_all(&self, oid: OID) -> RS<()> {
        let mut state = self.lock.lock()?;
        let mut wakes = Vec::new();
        state.tables.retain(|_, map| {
            map.retain(|_, entry| {
                if entry.owner.as_ref().is_some_and(|(owner, _)| *owner == oid) {
                    entry.owner = None;
                    if let Some(head) = entry.queue.front() {
                        wakes.push(head.clone());
                    }
                }
                entry.owner.is_some() || !entry.queue.is_empty()
            });
            !map.is_empty()
        });
        drop(state);
        self.send_wakes(wakes);
        Ok(())
    }

    /// Deliver targeted wakeups outside the lock hold. `ANotify` wakeups are
    /// sticky, so a waiter that has not parked yet still observes the signal
    /// when it does.
    fn send_wakes(&self, wakes: Vec<Arc<Waiter>>) {
        for waiter in wakes {
            self.wake_count.fetch_add(1, Ordering::Relaxed);
            waiter.notify.notify_waiters();
        }
    }
}

/// One all-or-nothing acquisition attempt against the locked state.
///
/// Orphan owners past `STATEMENT_LOCK_TTL` are reclaimed as a side effect;
/// queue heads behind a reclaimed owner are collected into `wakes`. When
/// `waiter` is `Some`, an unowned key may be taken only if the queue is
/// empty or the waiter sits at its head (FIFO grant); a `None` waiter (a
/// `try_lock_some` arrival) may take only keys with no waiters at all. On
/// success every requested key is owned by `oid` and `waiter` (if any) is
/// removed from all of their queues.
fn try_acquire_locked(
    state: &mut LockState,
    oid: OID,
    table_keys: &[(PhysicalRelationId, Vec<u8>)],
    waiter: Option<&Arc<Waiter>>,
    wakes: &mut Vec<Arc<Waiter>>,
) -> bool {
    // Check pass: decide acquirability, reclaiming orphans on the way.
    for (relation_id, key) in table_keys.iter() {
        let entry = state
            .tables
            .get_mut(relation_id)
            .and_then(|map| map.get_mut(key));
        if let Some(entry) = entry {
            if let Some((owner, acquired_at)) = &entry.owner {
                if *owner == oid {
                    // Re-entrant on a key we already hold.
                    continue;
                }
                if acquired_at.elapsed() <= STATEMENT_LOCK_TTL {
                    return false;
                }
                // Orphaned statement lock: the coordinator never came back.
                // Reclaim the key instead of blocking behind it forever.
                entry.owner = None;
                if let Some(head) = entry.queue.front() {
                    wakes.push(head.clone());
                }
            }
            let granted = match waiter {
                Some(waiter) => {
                    entry.queue.is_empty()
                        || entry
                            .queue
                            .front()
                            .is_some_and(|head| head.seq == waiter.seq)
                }
                None => entry.queue.is_empty(),
            };
            if !granted {
                return false;
            }
        }
    }
    // Take pass: all keys are acquirable, so take them all.
    for (relation_id, key) in table_keys.iter() {
        let map = state.tables.entry(*relation_id).or_default();
        let entry = map.entry(key.clone()).or_insert_with(KeyLockEntry::new);
        if entry.owner.as_ref().is_none_or(|(owner, _)| *owner != oid) {
            entry.owner = Some((oid, *instant_now()));
        }
        if let Some(waiter) = waiter {
            entry.queue.retain(|queued| queued.seq != waiter.seq);
        }
    }
    true
}

/// Append `waiter` to the FIFO queue of every requested key not already
/// owned by `oid`, keeping its existing position where already registered so
/// retries preserve the original arrival order.
fn enqueue_locked(
    state: &mut LockState,
    oid: OID,
    table_keys: &[(PhysicalRelationId, Vec<u8>)],
    waiter: &Arc<Waiter>,
) {
    for (relation_id, key) in table_keys.iter() {
        let map = state.tables.entry(*relation_id).or_default();
        if map
            .get(key)
            .and_then(|entry| entry.owner.as_ref())
            .is_some_and(|(owner, _)| *owner == oid)
        {
            continue;
        }
        let entry = map.entry(key.clone()).or_insert_with(KeyLockEntry::new);
        if !entry.queue.iter().any(|queued| queued.seq == waiter.seq) {
            entry.queue.push_back(waiter.clone());
        }
    }
}

/// Remove the waiter identified by `waiter_seq` from the queue of every
/// requested key (timeout/leave path). Where the removed waiter was the head
/// of a queue whose key is unowned, the new head is collected for wakeup.
fn dequeue_locked(
    state: &mut LockState,
    table_keys: &[(PhysicalRelationId, Vec<u8>)],
    waiter_seq: u64,
) -> Vec<Arc<Waiter>> {
    let mut wakes = Vec::new();
    for (relation_id, key) in table_keys.iter() {
        let mut remove_relation = false;
        if let Some(map) = state.tables.get_mut(relation_id) {
            if let Some(entry) = map.get_mut(key) {
                let was_head = entry
                    .queue
                    .front()
                    .is_some_and(|head| head.seq == waiter_seq);
                entry.queue.retain(|queued| queued.seq != waiter_seq);
                if was_head && entry.owner.is_none() {
                    if let Some(head) = entry.queue.front() {
                        wakes.push(head.clone());
                    }
                }
                if entry.owner.is_none() && entry.queue.is_empty() {
                    map.remove(key);
                }
            }
            remove_relation = map.is_empty();
        }
        if remove_relation {
            state.tables.remove(relation_id);
        }
    }
    wakes
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::todo,
        clippy::unimplemented
    )]

    use super::XLockMgr;
    use ::xengine::tx_mgr::PhysicalRelationId;
    use mudu_sys::time::instant_now;
    use std::sync::atomic::Ordering;
    use std::time::Duration;

    #[test]
    fn try_lock_some_rolls_back_partial_acquire_on_conflict() {
        let mgr = XLockMgr::new();
        let r = PhysicalRelationId {
            table_id: 1,
            partition_id: 0,
        };

        let owner_a_keys = vec![(r, b"k2".to_vec())];
        assert!(mgr.try_lock_some(100, &owner_a_keys).unwrap());

        let owner_b_keys = vec![(r, b"k1".to_vec()), (r, b"k2".to_vec())];
        assert!(!mgr.try_lock_some(200, &owner_b_keys).unwrap());

        // If partial lock rollback works, k1 should not be leaked and owner C
        // can lock it.
        let owner_c_keys = vec![(r, b"k1".to_vec())];
        assert!(mgr.try_lock_some(300, &owner_c_keys).unwrap());
    }

    #[test]
    fn try_lock_some_allows_reentrant_same_owner_key() {
        let mgr = XLockMgr::new();
        let r = PhysicalRelationId {
            table_id: 2,
            partition_id: 0,
        };
        let keys = vec![(r, b"k1".to_vec()), (r, b"k1".to_vec())];
        assert!(mgr.try_lock_some(42, &keys).unwrap());
    }

    #[test]
    fn release_all_drops_every_lock_of_the_owner() {
        let mgr = XLockMgr::new();
        let r1 = PhysicalRelationId {
            table_id: 1,
            partition_id: 0,
        };
        let r2 = PhysicalRelationId {
            table_id: 2,
            partition_id: 0,
        };
        let keys = vec![(r1, b"k1".to_vec()), (r2, b"k2".to_vec())];
        assert!(mgr.try_lock_some(100, &keys).unwrap());
        mgr.release_all(100).unwrap();
        assert!(mgr.try_lock_some(200, &keys).unwrap());
    }

    #[test]
    fn lock_some_waits_for_holder_and_acquires_after_release() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let mgr = std::sync::Arc::new(XLockMgr::new());
            let r = PhysicalRelationId {
                table_id: 1,
                partition_id: 0,
            };
            let keys = vec![(r, b"k".to_vec())];
            assert!(mgr.try_lock_some(100, &keys).unwrap());

            let releaser = mgr.clone();
            let release_keys = keys.clone();
            tokio::spawn(async move {
                let _ = mudu_sys::task::async_::sleep(Duration::from_millis(10)).await;
                releaser.release(100, &release_keys).unwrap();
            });

            // The contender parks (async) and wins the lock once the holder
            // releases, instead of failing immediately.
            assert!(mgr
                .lock_some(200, &keys, Duration::from_millis(1000))
                .await
                .unwrap());
            assert!(!mgr.try_lock_some(300, &keys).unwrap());
            mgr.release(200, &keys).unwrap();
        })
        .unwrap()
    }

    #[test]
    fn lock_some_times_out_when_holder_never_releases() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let mgr = XLockMgr::new();
            let r = PhysicalRelationId {
                table_id: 1,
                partition_id: 0,
            };
            let keys = vec![(r, b"k".to_vec())];
            assert!(mgr.try_lock_some(100, &keys).unwrap());

            let started = instant_now();
            let acquired = mgr
                .lock_some(200, &keys, Duration::from_millis(20))
                .await
                .unwrap();
            assert!(!acquired);
            assert!(started.elapsed() >= Duration::from_millis(20));
        })
        .unwrap()
    }

    #[test]
    fn lock_some_cross_key_contention_cannot_deadlock() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let mgr = std::sync::Arc::new(XLockMgr::new());
            let r = PhysicalRelationId {
                table_id: 1,
                partition_id: 0,
            };
            let key_a = (r, b"a".to_vec());
            let key_b = (r, b"b".to_vec());
            // tx 100 holds a, tx 200 holds b; each then wants the full set.
            assert!(mgr
                .try_lock_some(100, std::slice::from_ref(&key_a))
                .unwrap());
            assert!(mgr
                .try_lock_some(200, std::slice::from_ref(&key_b))
                .unwrap());

            let both = vec![key_a.clone(), key_b.clone()];
            let (wait100, wait200) = futures::join!(
                mgr.lock_some(100, &both, Duration::from_millis(30)),
                mgr.lock_some(200, &both, Duration::from_millis(30)),
            );
            // Both bounded waits expire instead of hanging forever: waiters
            // hold nothing while parked, so the circular wait cannot persist.
            assert!(!wait100.unwrap());
            assert!(!wait200.unwrap());
            mgr.release(100, std::slice::from_ref(&key_a)).unwrap();
            mgr.release(200, std::slice::from_ref(&key_b)).unwrap();
        })
        .unwrap()
    }

    /// Number of waiters currently queued on `key` of `relation`.
    fn queued_waiters(mgr: &XLockMgr, relation: PhysicalRelationId, key: &[u8]) -> usize {
        let state = mgr.lock.lock().unwrap();
        state
            .tables
            .get(&relation)
            .and_then(|map| map.get(key))
            .map_or(0, |entry| entry.queue.len())
    }

    /// Wait until `expected` waiters are queued on the key, so spawned waiter
    /// tasks have a deterministic arrival order.
    async fn wait_for_queue(
        mgr: &XLockMgr,
        relation: PhysicalRelationId,
        key: &[u8],
        expected: usize,
    ) {
        let deadline = instant_now() + Duration::from_secs(5);
        loop {
            if queued_waiters(mgr, relation, key) == expected {
                return;
            }
            assert!(instant_now() < deadline, "waiter queue did not fill up");
            let _ = mudu_sys::task::async_::sleep(Duration::from_millis(1)).await;
        }
    }

    #[test]
    fn lock_some_grants_waiters_in_fifo_order() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let mgr = std::sync::Arc::new(XLockMgr::new());
            let r = PhysicalRelationId {
                table_id: 1,
                partition_id: 0,
            };
            let keys = vec![(r, b"k".to_vec())];
            assert!(mgr.try_lock_some(100, &keys).unwrap());

            const WAITERS: u128 = 5;
            let (order_tx, mut order_rx) = tokio::sync::mpsc::unbounded_channel();
            let mut handles = Vec::new();
            for i in 0..WAITERS {
                let oid = 200 + i;
                let mgr2 = mgr.clone();
                let keys2 = keys.clone();
                let order_tx2 = order_tx.clone();
                handles.push(tokio::spawn(async move {
                    assert!(mgr2
                        .lock_some(oid, &keys2, Duration::from_secs(5))
                        .await
                        .unwrap());
                    order_tx2.send(oid).unwrap();
                    // Hold briefly so an out-of-order grant cannot be masked
                    // by a same-instant release.
                    let _ = mudu_sys::task::async_::sleep(Duration::from_millis(10)).await;
                    mgr2.release(oid, &keys2).unwrap();
                }));
                // Fix the arrival order before spawning the next waiter.
                wait_for_queue(&mgr, r, b"k", (i + 1) as usize).await;
            }

            // Releasing the initial holder hands the key to the queue head;
            // each waiter then releases to the next one in FIFO order.
            mgr.release(100, &keys).unwrap();
            let mut order = Vec::new();
            while let Some(oid) = order_rx.recv().await {
                order.push(oid);
                if order.len() == WAITERS as usize {
                    break;
                }
            }
            for handle in handles {
                handle.await.unwrap();
            }
            let expected: Vec<u128> = (0..WAITERS).map(|i| 200 + i).collect();
            assert_eq!(order, expected);
        })
        .unwrap()
    }

    #[test]
    fn release_wakes_only_waiters_of_the_released_key() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let mgr = std::sync::Arc::new(XLockMgr::new());
            let r = PhysicalRelationId {
                table_id: 1,
                partition_id: 0,
            };
            let key_a = vec![(r, b"a".to_vec())];
            let key_b = vec![(r, b"b".to_vec())];
            assert!(mgr.try_lock_some(100, &key_a).unwrap());
            assert!(mgr.try_lock_some(100, &key_b).unwrap());

            // Two waiters park on key a.
            let mut handles = Vec::new();
            for (i, oid) in [200, 201].iter().enumerate() {
                let mgr2 = mgr.clone();
                let key_a2 = key_a.clone();
                let oid = *oid;
                handles.push(tokio::spawn(async move {
                    mgr2.lock_some(oid, &key_a2, Duration::from_secs(5)).await
                }));
                wait_for_queue(&mgr, r, b"a", i + 1).await;
            }

            // Releasing the unrelated key b must not wake anyone.
            let wakes_before = mgr.wake_count.load(Ordering::Relaxed);
            mgr.release(100, &key_b).unwrap();
            assert_eq!(mgr.wake_count.load(Ordering::Relaxed), wakes_before);
            // Both waiters on a are still parked.
            assert_eq!(queued_waiters(&mgr, r, b"a"), 2);

            // Releasing a wakes exactly one waiter: the queue head.
            mgr.release(100, &key_a).unwrap();
            assert_eq!(mgr.wake_count.load(Ordering::Relaxed), wakes_before + 1);
            assert!(handles.remove(0).await.unwrap().unwrap());
            // The second waiter becomes the head once the first releases.
            mgr.release(200, &key_a).unwrap();
            assert!(handles.remove(0).await.unwrap().unwrap());
            mgr.release(201, &key_a).unwrap();
        })
        .unwrap()
    }

    #[test]
    fn try_lock_some_does_not_barge_ahead_of_waiters() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let mgr = std::sync::Arc::new(XLockMgr::new());
            let r = PhysicalRelationId {
                table_id: 1,
                partition_id: 0,
            };
            let keys = vec![(r, b"k".to_vec())];
            assert!(mgr.try_lock_some(100, &keys).unwrap());

            let mgr2 = mgr.clone();
            let keys2 = keys.clone();
            let waiter =
                tokio::spawn(
                    async move { mgr2.lock_some(200, &keys2, Duration::from_secs(5)).await },
                );
            wait_for_queue(&mgr, r, b"k", 1).await;

            // The key is now unowned but has a parked waiter: a fresh try_
            // must fail instead of jumping the FIFO queue. (If the woken
            // head wins first, try_ also fails because the key is owned
            // again — either way the newcomer cannot take it.)
            mgr.release(100, &keys).unwrap();
            assert!(!mgr.try_lock_some(300, &keys).unwrap());
            assert!(waiter.await.unwrap().unwrap());
            mgr.release(200, &keys).unwrap();
        })
        .unwrap()
    }

    #[test]
    fn timed_out_head_does_not_stall_the_queue() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let mgr = std::sync::Arc::new(XLockMgr::new());
            let r = PhysicalRelationId {
                table_id: 1,
                partition_id: 0,
            };
            let keys = vec![(r, b"k".to_vec())];
            assert!(mgr.try_lock_some(100, &keys).unwrap());

            // Head waiter with a short timeout, second waiter patient.
            let mgr1 = mgr.clone();
            let keys1 = keys.clone();
            let head = tokio::spawn(async move {
                mgr1.lock_some(200, &keys1, Duration::from_millis(30)).await
            });
            wait_for_queue(&mgr, r, b"k", 1).await;
            let mgr2 = mgr.clone();
            let keys2 = keys.clone();
            let second =
                tokio::spawn(
                    async move { mgr2.lock_some(201, &keys2, Duration::from_secs(5)).await },
                );
            wait_for_queue(&mgr, r, b"k", 2).await;

            // The head times out and leaves the queue.
            assert!(!head.await.unwrap().unwrap());
            wait_for_queue(&mgr, r, b"k", 1).await;

            // The release then goes to the new head, which acquires.
            mgr.release(100, &keys).unwrap();
            assert!(second.await.unwrap().unwrap());
            mgr.release(201, &keys).unwrap();
        })
        .unwrap()
    }
}

/// Deterministic-simulation model-check tests for the lock manager's waiting
/// path. Only compiled when the internal `ds` overlay wires `mudu_sys` to the
/// simulator backend (`-F mudu_kernel/ds`); inert in the public build.
#[cfg(all(test, feature = "ds"))]
mod ds_model_check_tests {
    use super::XLockMgr;
    use ::xengine::tx_mgr::PhysicalRelationId;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    /// Hold a sealed `Simulation` + `SimState` so the lock manager's
    /// `SMutex`/`ANotify` run on the deterministic backend.
    struct SealedEnv {
        _sim: mudu_sys::simulation::CurrentSimulationGuard,
        _state: mudu_sys::imp::runtime::SimStateGuard,
    }

    fn sealed_env() -> SealedEnv {
        let sim = mudu_sys::simulation::Simulation::new();
        let sim_guard = sim.make_current();
        let state = Arc::new(mudu_sys::imp::runtime::SimState::new());
        let state_guard = state.make_current();
        SealedEnv {
            _sim: sim_guard,
            _state: state_guard,
        }
    }

    /// Run a model check and report the explored state space to stdout, so a
    /// run shows how deep the exploration went (and whether it exhausted its
    /// budget). Use `-- --nocapture` to see it live.
    fn run_and_report<F, P>(
        name: &str,
        config: mudu_sys::simulation::ModelCheckerConfig,
        init: F,
        property: P,
    ) -> mudu_sys::simulation::ModelCheckResult
    where
        F: Fn() + 'static,
        P: Fn() -> Option<String> + 'static,
    {
        let result =
            mudu_sys::simulation::run_model_check_with_config(name, config, init, property);
        report_model_check(name, &result);
        result
    }

    /// `run_and_report` with a state fingerprint: the DFS deduplicates
    /// states across schedules by hashing the observable shared data that
    /// `fingerprint` reads.
    fn run_and_report_fingerprint<F, P, G>(
        name: &str,
        config: mudu_sys::simulation::ModelCheckerConfig,
        init: F,
        property: P,
        fingerprint: G,
    ) -> mudu_sys::simulation::ModelCheckResult
    where
        F: Fn() + 'static,
        P: Fn() -> Option<String> + 'static,
        G: Fn() -> u64 + 'static,
    {
        let result = mudu_sys::simulation::run_model_check_with_fingerprint(
            name,
            config,
            init,
            property,
            Some(fingerprint),
        );
        report_model_check(name, &result);
        result
    }

    /// Report the explored state space of a finished model check to stdout,
    /// so a run shows how deep the exploration went (and whether it
    /// exhausted its budget).
    fn report_model_check(name: &str, result: &mudu_sys::simulation::ModelCheckResult) {
        match result {
            mudu_sys::simulation::ModelCheckResult::Ok { schedules, visited } => {
                println!("[model-check:{name}] ok — {schedules} schedules, {visited} states");
            }
            mudu_sys::simulation::ModelCheckResult::Exhausted { schedules, visited } => {
                println!(
                    "[model-check:{name}] EXHAUSTED — {schedules} schedules, {visited} states"
                );
            }
            mudu_sys::simulation::ModelCheckResult::Counterexample(cx) => {
                println!("[model-check:{name}] COUNTEREXAMPLE — {}", cx.message);
            }
        }
    }

    /// Model-check the `lock_some` waiting path: a contender parked on the key
    /// must always acquire after the holder releases (no lost wakeup), in every
    /// explored schedule. A lost wakeup would leave the contender hung (the
    /// schedule exhausts `max_depth`) or timed out (`acquired == 0`), both of
    /// which fail the `Ok` assertion.
    #[test]
    fn lock_some_contender_acquires_after_release() {
        let _env = sealed_env();

        let relation = PhysicalRelationId {
            table_id: 1,
            partition_id: 0,
        };
        let keys = vec![(relation, b"k".to_vec())];
        let acquired = Arc::new(AtomicU32::new(0));

        let k = keys.clone();
        let a = acquired.clone();
        let init = move || {
            a.store(0, Ordering::Relaxed);
            let mgr = Arc::new(XLockMgr::new());

            // Holder takes the key.
            assert!(mgr.try_lock_some(100, &k).unwrap());

            // Contender waits (parks on its ANotify), then acquires and releases.
            let m = mgr.clone();
            let k_contender = k.clone();
            let a = a.clone();
            mudu_sys::task::spawn_task_named("contender", move || async move {
                let ok = m
                    .lock_some(200, &k_contender, Duration::from_secs(60))
                    .await
                    .unwrap();
                if ok {
                    a.fetch_add(1, Ordering::Relaxed);
                    m.release(200, &k_contender).unwrap();
                }
            });

            // Holder releases via a scheduled task, so the scheduler can order it
            // after the contender has queued up.
            let m = mgr.clone();
            let k_releaser = k.clone();
            mudu_sys::task::spawn_task_named("releaser", move || async move {
                m.release(100, &k_releaser).unwrap();
            });
        };

        let a = acquired.clone();
        let property = move || {
            if mudu_sys::simulation::get_simulator().ready_count() == 0 {
                let n = a.load(Ordering::Relaxed);
                if n != 1 {
                    return Some(format!("contender acquired {n} times"));
                }
            }
            None
        };

        let result = run_and_report(
            "xlock-no-lost-wakeup",
            mudu_sys::simulation::ModelCheckerConfig::default(),
            init,
            property,
        );
        assert!(
            matches!(result, mudu_sys::simulation::ModelCheckResult::Ok { .. }),
            "expected clean completion (no lost wakeup), got {result:?}"
        );
    }

    /// Model-check mutual exclusion: with N contenders on the same key, at most
    /// one transaction holds it at any time, in every explored schedule.
    #[test]
    fn lock_some_mutual_exclusion() {
        let _env = sealed_env();

        let relation = PhysicalRelationId {
            table_id: 1,
            partition_id: 0,
        };
        let key = vec![(relation, b"k".to_vec())];

        // `holders` counts currently-held locks; `max_holders` records the peak.
        let holders = Arc::new(AtomicU32::new(0));
        let max_holders = Arc::new(AtomicU32::new(0));

        let k = key.clone();
        let h = holders.clone();
        let mx = max_holders.clone();
        let init = move || {
            h.store(0, Ordering::Relaxed);
            mx.store(0, Ordering::Relaxed);
            let mgr = Arc::new(XLockMgr::new());

            for oid in [100u128, 101, 102] {
                let m = mgr.clone();
                let k = k.clone();
                let h = h.clone();
                let mx = mx.clone();
                mudu_sys::task::spawn_task_named(format!("tx-{oid}"), move || async move {
                    let ok = m.lock_some(oid, &k, Duration::from_secs(60)).await.unwrap();
                    if ok {
                        let n = h.fetch_add(1, Ordering::Relaxed) + 1;
                        mx.fetch_max(n, Ordering::Relaxed);
                        // Hold briefly so another transaction contends while this
                        // one still owns the key.
                        mudu_sys::task::get_current_task()
                            .unwrap()
                            .yield_now()
                            .await;
                        h.fetch_sub(1, Ordering::Relaxed);
                        m.release(oid, &k).unwrap();
                    }
                });
            }
        };

        let mx = max_holders.clone();
        let property = move || {
            if mudu_sys::simulation::get_simulator().ready_count() == 0 {
                let max = mx.load(Ordering::Relaxed);
                if max > 1 {
                    return Some(format!(
                        "mutual exclusion violated: {max} simultaneous holders"
                    ));
                }
            }
            None
        };

        // Three async contenders yield a larger schedule space than the default
        // bounds cover, so widen the DFS budget.
        let config = mudu_sys::simulation::ModelCheckerConfig {
            k: 2,
            max_depth: 128,
            max_schedules: 20_000,
            suspend_delays: vec![1],
        };

        // Fingerprint the observable shared data so the DFS can deduplicate
        // states across schedules. (Soundness caveat: the lock table itself is
        // private; this hashes only the holder counters.)
        let holders_fp = holders.clone();
        let max_fp = max_holders.clone();
        let fingerprint = move || {
            let h = holders_fp.load(Ordering::Relaxed) as u64;
            let m = max_fp.load(Ordering::Relaxed) as u64;
            h.wrapping_mul(0x9E37_79B9).wrapping_add(m)
        };

        let result = run_and_report_fingerprint("xlock-mutex", config, init, property, fingerprint);
        assert!(
            matches!(result, mudu_sys::simulation::ModelCheckResult::Ok { .. }),
            "expected mutual exclusion to hold, got {result:?}"
        );
    }
}
