use crate::contract::snapshot::{RunningXList, Snapshot};
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_sys::sync::SMutex;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KvItem {
    pub key: Vec<u8>,
    pub value: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkerSnapshot {
    xid: u64,
    running: Vec<u64>,
}

pub struct WorkerSnapshotMgr {
    next_ts: AtomicU64,
    running: SMutex<Vec<u64>>,
}

impl WorkerSnapshot {
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
    pub(crate) fn latest_committed() -> Self {
        Self::new(u64::MAX - 1, vec![])
    }

    pub fn xid(&self) -> u64 {
        self.xid
    }

    pub fn is_visible(&self, version_xid: u64) -> bool {
        is_visible_to_snapshot(version_xid, self)
    }

    pub fn to_snapshot(&self) -> Snapshot {
        Snapshot::from(RunningXList::new(self.xid, self.running.clone()))
    }
}

impl WorkerSnapshotMgr {
    pub fn begin_tx(&self) -> RS<WorkerSnapshot> {
        let xid = self.next_ts.fetch_add(1, Ordering::Relaxed) + 1;
        let mut running = self.running.lock()?;
        let snapshot = WorkerSnapshot {
            xid,
            running: running.clone(),
        };
        insert_sorted_unique(&mut running, xid);
        Ok(snapshot)
    }

    pub fn alloc_committed_ts(&self) -> u64 {
        self.next_ts.fetch_add(1, Ordering::Relaxed) + 1
    }

    pub fn observe_committed_ts(&self, xid: u64) {
        self.next_ts.fetch_max(xid, Ordering::Relaxed);
    }

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
