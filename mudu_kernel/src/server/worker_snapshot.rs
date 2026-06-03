use crate::contract::snapshot::{RunningXList, Snapshot};
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use std::sync::atomic::{AtomicU64, Ordering};
use mudu_sys::sync::SMutex;

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
    pub fn begin_tx(&self) -> WorkerSnapshot {
        let xid = self.next_ts.fetch_add(1, Ordering::Relaxed) + 1;
        let mut running = self
            .running
            .lock()
            .expect("worker snapshot manager running list lock poisoned");
        let snapshot = WorkerSnapshot {
            xid,
            running: running.clone(),
        };
        insert_sorted_unique(&mut running, xid);
        snapshot
    }

    pub fn alloc_committed_ts(&self) -> u64 {
        self.next_ts.fetch_add(1, Ordering::Relaxed) + 1
    }

    pub fn observe_committed_ts(&self, xid: u64) {
        self.next_ts.fetch_max(xid, Ordering::Relaxed);
    }

    pub fn end_tx(&self, xid: u64) -> RS<()> {
        let mut running = self
            .running
            .lock()
            .expect("worker snapshot manager running list lock poisoned");
        match running.binary_search(&xid) {
            Ok(index) => {
                running.remove(index);
                Ok(())
            }
            Err(_) => Err(m_error!(
                EC::NoSuchElement,
                format!("transaction {} is not active", xid)
            )),
        }
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
