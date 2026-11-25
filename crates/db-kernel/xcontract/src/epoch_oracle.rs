//! `LocalEpochOracle`: the single-node epoch oracle. One instance is shared
//! by every worker of the node; the cluster `RaftEpochOracle` replaces it
//! when the epoch group lands (phase 2).
//!
//! Advancement rule ("advance at join after a drain"): transactions
//! register against the current epoch at begin and deregister at end; when
//! the active set drains, the drained epoch becomes stable, and the next
//! registration opens a fresh epoch (so consecutive commit batches stay
//! separated). On an idle oracle `stable == current`, so waiting for a
//! watermark captured at quiescence never hangs. Registration and
//! advancement are serialized on the same lock, so a transaction can never
//! join an epoch the barrier already crossed.

use async_trait::async_trait;
use catalog::contract::epoch_oracle::{CommitEpoch, EpochOracle};
use mudu::common::result::RS;
use mudu_sys::sync::SMutex;
use mudu_sys::tokio::sync::Notify;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// In-process epoch oracle for single-node deployments and tests.
pub struct LocalEpochOracle {
    /// The epoch currently collecting commits.
    current: AtomicU64,
    /// The newest quiesced epoch, safe for snapshot reads.
    stable: AtomicU64,
    /// Active transaction count per joined epoch.
    active: SMutex<BTreeMap<CommitEpoch, usize>>,
    /// Fires when `stable` advances (waiters of `wait_stable`).
    advanced: Notify,
}

impl LocalEpochOracle {
    /// Creates an oracle whose epochs start above the legacy-xid segment
    /// (`EPOCH_XID_BASE`): epoch-stamped versions never collide with
    /// xid-stamped versions written before the epoch upgrade.
    pub fn new() -> Arc<Self> {
        Self::new_at(catalog::contract::epoch_oracle::EPOCH_XID_BASE + 1)
    }

    /// Creates an oracle with its first collecting epoch at `base`
    /// (tests use small bases; production uses [`Self::new`]).
    pub fn new_at(base: CommitEpoch) -> Arc<Self> {
        Arc::new(Self {
            current: AtomicU64::new(base),
            stable: AtomicU64::new(base.saturating_sub(1)),
            active: SMutex::new(BTreeMap::new()),
            advanced: Notify::new(),
        })
    }

    /// Re-evaluates quiescence; called with the `active` lock held after
    /// every deregistration.
    fn advance_locked(&self, active: &BTreeMap<CommitEpoch, usize>) {
        // stable may advance to just below the oldest active epoch, or to the
        // current epoch when nothing is in flight (the collecting epoch of an
        // idle oracle is quiesced by definition; the next registration opens
        // a fresh one — see register_active).
        let new_stable = match active.first_key_value() {
            Some((oldest_active, _)) => oldest_active.saturating_sub(1),
            None => self.current.load(Ordering::Acquire),
        };
        if self.stable.fetch_max(new_stable, Ordering::AcqRel) < new_stable {
            self.advanced.notify_waiters();
        }
    }
}

impl Default for LocalEpochOracle {
    fn default() -> Self {
        Self {
            current: AtomicU64::new(1),
            stable: AtomicU64::new(0),
            active: SMutex::new(BTreeMap::new()),
            advanced: Notify::new(),
        }
    }
}

#[async_trait]
impl EpochOracle for LocalEpochOracle {
    fn current_epoch(&self) -> CommitEpoch {
        self.current.load(Ordering::Acquire)
    }

    fn stable_epoch(&self) -> CommitEpoch {
        self.stable.load(Ordering::Acquire)
    }

    async fn wait_stable(&self, e: CommitEpoch) -> RS<()> {
        loop {
            if self.stable_epoch() >= e {
                return Ok(());
            }
            // Poll with a timeout: a notify_waiters() landing between the
            // check and the wait registration would otherwise be missed
            // forever on an idle cluster.
            let _ = mudu_sys::timeout(
                std::time::Duration::from_millis(50),
                self.advanced.notified(),
            )
            .await;
        }
    }

    fn register_active(&self) -> CommitEpoch {
        match self.active.lock() {
            Ok(mut active) => {
                // Re-read under the lock: registration and advancement are
                // serialized here, so a transaction can never join an epoch
                // the barrier already crossed.
                if active.is_empty() {
                    // The previous batch fully drained (stable reached the
                    // collecting epoch): open a fresh epoch so this batch's
                    // commits stay separated from the batch just proven
                    // stable. Deferred to registration (rather than done at
                    // drain) so an idle oracle keeps stable == current and a
                    // wait on a quiescence watermark never hangs.
                    let current = self.current.load(Ordering::Acquire);
                    if self.stable.load(Ordering::Acquire) >= current {
                        self.current.fetch_max(current + 1, Ordering::AcqRel);
                    }
                }
                let epoch = self.current.load(Ordering::Acquire);
                *active.entry(epoch).or_insert(0) += 1;
                epoch
            }
            Err(_) => {
                tracing::error!("local epoch oracle active lock poisoned");
                self.current.load(Ordering::Acquire)
            }
        }
    }

    fn unregister_active(&self, epoch: CommitEpoch) {
        match self.active.lock() {
            Ok(mut active) => {
                match active.get_mut(&epoch) {
                    Some(count) => {
                        *count -= 1;
                        if *count == 0 {
                            active.remove(&epoch);
                        }
                    }
                    None => {
                        debug_assert!(false, "unregister of inactive epoch {}", epoch);
                    }
                }
                self.advance_locked(&active);
            }
            Err(_) => {
                tracing::error!("local epoch oracle active lock poisoned");
            }
        }
    }

    fn resume(&self, recovered: CommitEpoch) {
        // fetch_max keeps the shared (node-wide) oracle monotone when the
        // workers resume it one after another.
        self.current.fetch_max(recovered + 1, Ordering::AcqRel);
        if self.stable.fetch_max(recovered, Ordering::AcqRel) < recovered {
            self.advanced.notify_waiters();
        }
    }
}

/// The cluster-wide epoch oracle (phase 2): reads the shared epoch state the
/// node's epoch-group replica maintains, and tracks this node's active
/// epochs for the drained-epoch heartbeat.
pub struct ClusterEpochOracle {
    /// The node-shared epoch state maintained by the epoch group.
    state: Arc<catalog::contract::epoch_oracle::ClusterEpochState>,
    /// This node's active transactions per joined epoch.
    active: SMutex<BTreeMap<CommitEpoch, usize>>,
}

impl ClusterEpochOracle {
    /// Creates the oracle over the node's shared epoch state.
    pub fn new(state: Arc<catalog::contract::epoch_oracle::ClusterEpochState>) -> Arc<Self> {
        Arc::new(Self {
            state,
            active: SMutex::new(BTreeMap::new()),
        })
    }

    /// Samples closed and drained prefixes under the transaction-registration
    /// lock. An idle node cannot attest that its still-open epoch is drained.
    pub fn closed_drained_epochs(&self) -> RS<(CommitEpoch, CommitEpoch)> {
        let active = self.active.lock()?;
        let current = self.state.current.load(Ordering::Acquire);
        let closed = current.saturating_sub(1);
        // A fully drained node attests the live edge itself (there is no
        // still-active transaction to pin the barrier below it); otherwise
        // the barrier is pinned just below the oldest active epoch.
        let drained = active
            .first_key_value()
            .map_or(current, |(oldest, _)| oldest.saturating_sub(1));
        Ok((closed, drained))
    }
}

#[async_trait]
impl EpochOracle for ClusterEpochOracle {
    fn current_epoch(&self) -> CommitEpoch {
        self.state.current.load(Ordering::Acquire)
    }

    fn stable_epoch(&self) -> CommitEpoch {
        self.state.stable.load(Ordering::Acquire)
    }

    async fn wait_stable(&self, e: CommitEpoch) -> RS<()> {
        loop {
            if self.stable_epoch() >= e {
                return Ok(());
            }
            let _ = mudu_sys::timeout(
                std::time::Duration::from_millis(50),
                self.state.stable_notify.notified(),
            )
            .await;
        }
    }

    fn register_active(&self) -> CommitEpoch {
        match self.active.lock() {
            Ok(mut active) => {
                let epoch = self.current_epoch();
                *active.entry(epoch).or_insert(0) += 1;
                epoch
            }
            Err(_) => {
                tracing::error!("cluster epoch oracle active lock poisoned");
                self.current_epoch()
            }
        }
    }

    fn unregister_active(&self, epoch: CommitEpoch) {
        if let Ok(mut active) = self.active.lock() {
            if let Some(count) = active.get_mut(&epoch) {
                *count -= 1;
                if *count == 0 {
                    active.remove(&epoch);
                }
            }
        }
    }

    fn resume(&self, recovered: CommitEpoch) {
        // The epoch group's raft log replays its own authoritative advances;
        // resume the node-local view so reads issued before the first
        // post-restart heartbeat still observe pre-shutdown commits. The
        // dispatch applies with fetch_max, so the view never regresses.
        self.state
            .current
            .fetch_max(recovered + 1, Ordering::AcqRel);
        if self.state.stable.fetch_max(recovered, Ordering::AcqRel) < recovered {
            self.state.stable_notify.notify_waiters();
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn idle_advance_makes_commits_stable() {
        let oracle = LocalEpochOracle::new_at(1);
        assert_eq!(oracle.current_epoch(), 1);
        assert_eq!(oracle.stable_epoch(), 0);

        // A transaction in flight pins the barrier below its epoch.
        let e1 = oracle.register_active();
        assert_eq!(e1, 1);
        let e2 = oracle.register_active();
        assert_eq!(e2, 1);
        oracle.unregister_active(e1);
        assert_eq!(oracle.stable_epoch(), 0);
        oracle.unregister_active(e2);
        // Drained: epoch 1 is stable. The collecting epoch stays at 1 until
        // the next registration opens epoch 2, so an idle oracle keeps
        // stable == current (a wait on a quiescence watermark never hangs).
        assert_eq!(oracle.stable_epoch(), 1);
        assert_eq!(oracle.current_epoch(), 1);

        let e3 = oracle.register_active();
        assert_eq!(e3, 2);
        // A fresh registration bars the barrier from crossing epoch 2 while
        // epoch 1 stays stable.
        assert_eq!(oracle.stable_epoch(), 1);
        oracle.unregister_active(e3);
        assert_eq!(oracle.stable_epoch(), 2);
    }

    #[test]
    fn production_oracle_starts_above_the_legacy_segment() {
        let oracle = LocalEpochOracle::new();
        assert!(
            oracle.current_epoch() > catalog::contract::epoch_oracle::EPOCH_XID_BASE,
            "epochs must not collide with legacy xids"
        );
    }

    #[test]
    fn resume_restarts_above_recovered_epochs() {
        let oracle = LocalEpochOracle::new_at(1);
        // WAL replay found commits stamped up to epoch 7: the barrier jumps
        // to 7 (all drained by definition) and the collecting epoch resumes
        // at 8.
        oracle.resume(7);
        assert_eq!(oracle.stable_epoch(), 7);
        assert_eq!(oracle.current_epoch(), 8);
        let joined = oracle.register_active();
        assert_eq!(joined, 8);
        oracle.unregister_active(joined);
        assert!(oracle.stable_epoch() >= 8);
        // Monotone: a stale resume never regresses the oracle.
        oracle.resume(3);
        assert!(oracle.current_epoch() >= 8);
        assert!(oracle.stable_epoch() >= 8);
    }

    #[test]
    fn cluster_reports_attest_the_live_edge_when_drained() {
        let state = catalog::contract::epoch_oracle::ClusterEpochState::new();
        let oracle = ClusterEpochOracle::new(state.clone());
        let live = oracle.current_epoch();
        // No active transaction: the node attests the live edge is drained.
        assert_eq!(oracle.closed_drained_epochs().unwrap(), (live - 1, live));
        let active = oracle.register_active();
        state.current.store(live + 2, Ordering::Release);
        // An active registration pins the barrier below its epoch.
        assert_eq!(
            oracle.closed_drained_epochs().unwrap(),
            (live + 1, live - 1)
        );
        oracle.unregister_active(active);
        // Drained again: the node attests the new live edge.
        assert_eq!(
            oracle.closed_drained_epochs().unwrap(),
            (live + 1, live + 2)
        );
        // An idle report remains safe when a transaction registers afterwards.
        assert_eq!(oracle.register_active(), live + 2);
        assert_eq!(
            oracle.closed_drained_epochs().unwrap(),
            (live + 1, live + 1)
        );
    }

    #[test]
    fn wait_stable_wakes_on_advance() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let oracle = LocalEpochOracle::new_at(1);
            let joined = oracle.register_active();
            let waiter = {
                let oracle = oracle.clone();
                mudu_sys::tokio::spawn(async move { oracle.wait_stable(joined).await })
            };
            // Let the waiter park, then drain the epoch.
            mudu_utils::yield_now::cooperative_yield_now().await;
            assert_eq!(oracle.stable_epoch(), 0);
            oracle.unregister_active(joined);
            waiter.await.unwrap().unwrap();
            assert!(oracle.stable_epoch() >= joined);
            Ok::<(), mudu::error::MuduError>(())
        })
        .unwrap()
        .unwrap();
    }
}
