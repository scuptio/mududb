use crate::contract::snapshot::{RunningXList, Snapshot};
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_sys::sync::SMutex;
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
}

/// Allocates transaction ids and tracks the running transactions of a worker.
pub struct WorkerSnapshotMgr {
    next_ts: AtomicU64,
    running: SMutex<Vec<u64>>,
    /// Raft term source in replication mode: xids become
    /// `(term << 32) | counter` so a new leader never reissues another
    /// leader's transaction ids.
    raft_term: SMutex<Option<Arc<AtomicU64>>>,
}

impl WorkerSnapshot {
    /// Creates a snapshot for `xid` with the given sorted running-xid list.
    pub fn new(xid: u64, running: Vec<u64>) -> Self {
        Self { xid, running }
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

    /// Whether a version committed by `version_xid` is visible to this
    /// snapshot.
    pub fn is_visible(&self, version_xid: u64) -> bool {
        is_visible_to_snapshot(version_xid, self)
    }

    /// Converts into the shared [`Snapshot`] visibility type.
    pub fn to_snapshot(&self) -> Snapshot {
        Snapshot::from(RunningXList::new(self.xid, self.running.clone()))
    }
}

impl WorkerSnapshotMgr {
    /// Installs the raft term source (replication mode).
    pub fn set_raft_term_source(&self, term: Arc<AtomicU64>) {
        if let Ok(mut slot) = self.raft_term.lock() {
            *slot = Some(term);
        }
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
    pub fn begin_tx(&self) -> RS<WorkerSnapshot> {
        let xid = self.alloc_xid();
        let mut running = self.running.lock()?;
        let snapshot = WorkerSnapshot {
            xid,
            running: running.clone(),
        };
        insert_sorted_unique(&mut running, xid);
        Ok(snapshot)
    }

    /// Allocates a commit timestamp.
    pub fn alloc_committed_ts(&self) -> u64 {
        self.alloc_xid()
    }

    /// Advances the allocation watermark past `xid` when it is newer (e.g. a
    /// replicated commit timestamp).
    pub fn observe_committed_ts(&self, xid: u64) {
        self.next_ts.fetch_max(xid, Ordering::Relaxed);
    }

    /// Removes `xid` from the running set; errors when it is not active.
    pub fn end_tx(&self, xid: u64) -> RS<()> {
        let mut running = self.running.lock()?;
        match running.binary_search(&xid) {
            Ok(index) => {
                running.remove(index);
                Ok(())
            }
            Err(_) => Err(mudu_error!(
                ErrorCode::EntityNotFound,
                format!("transaction {} is not active", xid)
            )),
        }
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
        }
    }
}

fn is_visible_to_snapshot(version_xid: u64, snapshot: &WorkerSnapshot) -> bool {
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
