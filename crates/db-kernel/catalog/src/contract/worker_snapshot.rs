use crate::contract::epoch_oracle::{CommitEpoch, EpochOracle, EPOCH_XID_BASE};
use crate::contract::snapshot::{RunningXList, Snapshot};
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_sys::sync::SMutex;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// A key/value pair.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KvItem {
    /// Key bytes.
    pub key: Vec<u8>,
    /// Value bytes.
    pub value: Vec<u8>,
}

/// A worker's view of the running transactions when its transaction began.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkerSnapshot {
    xid: u64,
    running: Vec<u64>,
    /// The epoch the transaction joined, when an epoch oracle is installed.
    epoch: Option<CommitEpoch>,
    /// The stable epoch captured at begin (read snapshot), when an epoch
    /// oracle is installed.
    read_epoch: Option<CommitEpoch>,
}

/// Allocates transaction ids and tracks the running transactions of a worker.
pub struct WorkerSnapshotMgr {
    next_ts: AtomicU64,
    running: SMutex<Vec<u64>>,
    /// Raft term source in replication mode: xids become
    /// `(term << 32) | counter` so a new leader never reissues another
    /// leader's transaction ids.
    raft_term: SMutex<Option<Arc<AtomicU64>>>,
    /// Epoch oracle (the commit-order source); installed by the server, one
    /// shared per node. `None` keeps the pre-epoch behavior.
    epoch_oracle: SMutex<Option<Arc<dyn EpochOracle>>>,
    /// xid → joined epoch of the running transactions, used to deregister
    /// them from the oracle at `end_tx` (populated only with an oracle).
    active_epochs: SMutex<HashMap<u64, CommitEpoch>>,
    /// The highest commit epoch replayed into this worker's storage (the
    /// replica's applied watermark; 0 on a leader).
    applied_commit_epoch: AtomicU64,
}

impl WorkerSnapshot {
    /// Creates a snapshot for `xid` with the given sorted running-xid list.
    pub fn new(xid: u64, running: Vec<u64>) -> Self {
        Self {
            xid,
            running,
            epoch: None,
            read_epoch: None,
        }
    }

    /// A snapshot that observes every version committed so far. Used to
    /// re-read a row under a statement-level write lock the caller already
    /// holds: there the begin-time snapshot would be needlessly stale (the
    /// lock, not the snapshot, is the conflict protection for that row).
    ///
    /// `u64::MAX - 1` stays below the `c_max = u64::MAX` sentinel used by
    /// version timestamps, so every committed version is visible to it.
    pub fn latest_committed() -> Self {
        Self::new(u64::MAX - 1, vec![])
    }

    /// The snapshot's transaction id.
    pub fn xid(&self) -> u64 {
        self.xid
    }

    /// The epoch the transaction joined, when an epoch oracle is installed.
    pub fn epoch(&self) -> Option<CommitEpoch> {
        self.epoch
    }

    /// The stable read epoch captured at begin, when an epoch oracle is
    /// installed.
    pub fn read_epoch(&self) -> Option<CommitEpoch> {
        self.read_epoch
    }

    /// A synthetic snapshot for validation-only reads at `read_epoch` (the
    /// two-phase-commit prepare conflict check reads through it).
    pub fn new_with_read_epoch(read_epoch: CommitEpoch) -> Self {
        Self {
            xid: 0,
            running: vec![],
            epoch: None,
            read_epoch: Some(read_epoch),
        }
    }

    /// Whether a version committed by `version_xid` is visible to this
    /// snapshot.
    pub fn is_visible(&self, version_xid: u64) -> bool {
        is_visible_to_snapshot(version_xid, self)
    }

    /// Converts into the shared [`Snapshot`] visibility type.
    pub fn to_snapshot(&self) -> Snapshot {
        // The visibility mode keys off the read epoch, not the joined
        // epoch: untracked statement shells (`begin_tx_untracked`) carry a
        // read epoch without a registration and read epoch-stamped versions
        // like any epoch transaction.
        match self.read_epoch {
            Some(read_epoch) => Snapshot::from(RunningXList::new_with_epoch(
                self.xid,
                self.running.clone(),
                read_epoch,
            )),
            None => Snapshot::from(RunningXList::new(self.xid, self.running.clone())),
        }
    }
}

impl WorkerSnapshotMgr {
    /// Installs the raft term source (replication mode).
    pub fn set_raft_term_source(&self, term: Arc<AtomicU64>) {
        if let Ok(mut slot) = self.raft_term.lock() {
            *slot = Some(term);
        }
    }

    /// Installs the epoch oracle (the commit-order source); one shared per
    /// node, set on every worker's snapshot manager at startup.
    pub fn set_epoch_oracle(&self, oracle: Arc<dyn EpochOracle>) {
        if let Ok(mut slot) = self.epoch_oracle.lock() {
            *slot = Some(oracle);
        }
    }

    /// The installed epoch oracle, if any.
    pub fn epoch_oracle(&self) -> Option<Arc<dyn EpochOracle>> {
        self.epoch_oracle.lock().ok()?.clone()
    }

    /// Allocates the next xid. Single-node: a plain counter. Replication
    /// mode: `(raft_term << 32) | counter`, monotonic within a term and
    /// jumping ahead whenever the term advances, so leaders of different
    /// terms never collide on xids. `next_ts` only ever moves forward.
    fn alloc_xid(&self) -> u64 {
        let term = self
            .raft_term
            .lock()
            .ok()
            .and_then(|slot| slot.as_ref().map(|term| term.load(Ordering::Relaxed)));
        let Some(term) = term else {
            return self.next_ts.fetch_add(1, Ordering::Relaxed) + 1;
        };
        let base = term << 32;
        loop {
            let current = self.next_ts.load(Ordering::Relaxed);
            let candidate = if current >= base {
                current + 1
            } else {
                base + 1
            };
            if self
                .next_ts
                .compare_exchange(current, candidate, Ordering::Relaxed, Ordering::Relaxed)
                .is_ok()
            {
                return candidate;
            }
        }
    }

    /// Allocates a new xid, registers it as running, and returns its snapshot.
    /// With an epoch oracle installed, the transaction also joins the
    /// oracle's current epoch (barring the barrier from crossing it) and
    /// captures the stable epoch as its read snapshot.
    pub fn begin_tx(&self) -> RS<WorkerSnapshot> {
        let xid = self.alloc_xid();
        let mut running = self.running.lock()?;
        let (epoch, read_epoch) = match self.epoch_oracle.lock()?.as_ref() {
            Some(oracle) => {
                let read_epoch = oracle.stable_epoch();
                (Some(oracle.register_active()), Some(read_epoch))
            }
            None => (None, None),
        };
        let snapshot = WorkerSnapshot {
            xid,
            running: running.clone(),
            epoch,
            read_epoch,
        };
        if let Some(epoch) = epoch {
            self.active_epochs.lock()?.insert(xid, epoch);
        }
        insert_sorted_unique(&mut running, xid);
        Ok(snapshot)
    }

    /// Like [`WorkerSnapshotMgr::begin_tx`], but the read snapshot is the
    /// current epoch instead of the stable one: versions committed at the
    /// live edge are visible. For control-plane re-homing under an
    /// exclusive partition fence (phase 3), where "committed at the live
    /// edge" is provably committed (the fence blocks new writes).
    pub fn begin_tx_reading_current(&self) -> RS<WorkerSnapshot> {
        let xid = self.alloc_xid();
        let mut running = self.running.lock()?;
        let (epoch, read_epoch) = match self.epoch_oracle.lock()?.as_ref() {
            Some(oracle) => {
                let read_epoch = oracle.current_epoch();
                (Some(oracle.register_active()), Some(read_epoch))
            }
            None => (None, None),
        };
        let snapshot = WorkerSnapshot {
            xid,
            running: running.clone(),
            epoch,
            read_epoch,
        };
        if let Some(epoch) = epoch {
            self.active_epochs.lock()?.insert(xid, epoch);
        }
        insert_sorted_unique(&mut running, xid);
        Ok(snapshot)
    }

    /// Begins a statement-shell transaction that does NOT register with the
    /// epoch oracle: for autocommit statements whose real work runs in
    /// autonomous inner transactions (queue `RECEIVE ... WAIT`), where a
    /// held registration would pin the collecting epoch for the whole park
    /// — producers' commits would stamp the pinned epoch and stay invisible
    /// to the waiter's rounds, and their later read-your-writes waits would
    /// hang on the parked statement. The shell reads at the stable epoch
    /// like a regular transaction and must stay empty: the commit path
    /// rejects a non-empty untracked transaction (it has no pinned epoch to
    /// stamp with).
    pub fn begin_tx_untracked(&self) -> RS<WorkerSnapshot> {
        let xid = self.alloc_xid();
        let mut running = self.running.lock()?;
        let read_epoch = self
            .epoch_oracle
            .lock()?
            .as_ref()
            .map(|oracle| oracle.stable_epoch());
        let snapshot = WorkerSnapshot {
            xid,
            running: running.clone(),
            epoch: None,
            read_epoch,
        };
        insert_sorted_unique(&mut running, xid);
        Ok(snapshot)
    }

    /// Allocates a commit timestamp.
    pub fn alloc_committed_ts(&self) -> u64 {
        self.alloc_xid()
    }

    /// Notes a replayed batch's commit epoch (the replica's applied
    /// watermark for follower reads).
    pub fn note_applied_commit_epoch(&self, epoch: CommitEpoch) {
        self.applied_commit_epoch
            .fetch_max(epoch, Ordering::Relaxed);
    }

    /// The replica's applied commit watermark (0 on a leader).
    pub fn applied_commit_epoch(&self) -> CommitEpoch {
        self.applied_commit_epoch.load(Ordering::Relaxed)
    }

    /// Advances the allocation watermark past `xid` when it is newer (e.g. a
    /// replicated commit timestamp).
    pub fn observe_committed_ts(&self, xid: u64) {
        self.next_ts.fetch_max(xid, Ordering::Relaxed);
    }

    /// Removes `xid` from the running set; errors when it is not active.
    /// With an epoch oracle installed, the transaction also leaves its epoch
    /// (which may advance the barrier).
    pub fn end_tx(&self, xid: u64) -> RS<()> {
        let mut running = self.running.lock()?;
        match running.binary_search(&xid) {
            Ok(index) => {
                running.remove(index);
            }
            Err(_) => {
                return Err(mudu_error!(
                    ErrorCode::ObjectNotFound,
                    format!("transaction {} is not active", xid)
                ))
            }
        }
        drop(running);
        if let Some(epoch) = self.active_epochs.lock()?.remove(&xid) {
            if let Some(oracle) = self.epoch_oracle.lock()?.as_ref() {
                oracle.unregister_active(epoch);
            }
        }
        Ok(())
    }

    /// Return the oldest xid still running, if any.
    pub fn oldest_running_xid(&self) -> RS<Option<u64>> {
        Ok(self.running.lock()?.first().copied())
    }

    /// Return the newest xid allocated so far (begin or commit timestamp);
    /// `0` when nothing was allocated yet.
    pub fn latest_xid(&self) -> u64 {
        self.next_ts.load(Ordering::Relaxed)
    }
}

impl Default for WorkerSnapshotMgr {
    fn default() -> Self {
        Self {
            next_ts: AtomicU64::new(0),
            running: SMutex::new(Vec::new()),
            raft_term: SMutex::new(None),
            epoch_oracle: SMutex::new(None),
            active_epochs: SMutex::new(HashMap::new()),
            applied_commit_epoch: AtomicU64::new(0),
        }
    }
}

fn is_visible_to_snapshot(version_xid: u64, snapshot: &WorkerSnapshot) -> bool {
    if let Some(read_epoch) = snapshot.read_epoch {
        // Epoch mode: the stamp is a commit epoch; legacy xid stamps
        // (< EPOCH_XID_BASE, all committed pre-upgrade) are always visible.
        return version_xid < EPOCH_XID_BASE || version_xid <= read_epoch;
    }
    if version_xid > snapshot.xid {
        return false;
    }
    snapshot.running.binary_search(&version_xid).is_err()
}

fn insert_sorted_unique(values: &mut Vec<u64>, value: u64) {
    match values.binary_search(&value) {
        Ok(_) => {}
        Err(index) => values.insert(index, value),
    }
}
