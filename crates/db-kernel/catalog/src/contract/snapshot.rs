use crate::contract::timestamp::Timestamp;
use mudu::common::id::OID;
use std::sync::Arc;

/// Logical timestamp / transaction-id sequence value.
pub type TimeSeq = u64;

/// The set of transactions still running when a snapshot was taken.
#[derive(Debug, Clone)]
pub struct RunningXList {
    creator_ts: u64,
    running: Vec<u64>,
}

/// MVCC visibility snapshot: decides which tuple versions a transaction sees.
#[derive(Debug, Clone)]
pub struct Snapshot {
    inner: Arc<_Snapshot>,
}

#[derive(Debug, Clone)]
struct _Snapshot {
    creator_ts: TimeSeq,
    lower_un_alloc_ts: TimeSeq,
    upper_fin_ts: TimeSeq,
    running: Vec<TimeSeq>,
}

impl Snapshot {
    /// Builds a snapshot from a running-transaction list.
    pub fn from(snapshot: RunningXList) -> Self {
        Self {
            inner: Arc::new(_Snapshot::from(snapshot)),
        }
    }

    /// The timestamp of the transaction that created this snapshot.
    pub fn xid(&self) -> OID {
        self.inner.xid() as _
    }

    /// Whether a change with commit timestamp `xid` is visible.
    pub fn is_visible(&self, xid: TimeSeq) -> bool {
        self.inner.is_visible(xid)
    }

    /// Whether the tuple version stamped `tuple_ts` is visible.
    pub fn is_tuple_visible(&self, tuple_ts: &Timestamp) -> bool {
        self.inner.is_tuple_visible(tuple_ts)
    }
}

impl RunningXList {
    /// Creates the list from the creator's timestamp and the still-running
    /// transaction timestamps (sorted).
    pub fn new(creator_ts: u64, running: Vec<u64>) -> RunningXList {
        Self {
            creator_ts,
            running,
        }
    }
}

impl _Snapshot {
    fn from(snapshot: RunningXList) -> Self {
        let running = snapshot.running;
        let creator_ts = snapshot.creator_ts;
        assert!(running.is_sorted());
        let low_ts = match running.first() {
            None => creator_ts,
            Some(v) => *v,
        };
        let upper_ts = match running.last() {
            None => creator_ts,
            Some(v) => *v,
        };
        Self {
            creator_ts,
            lower_un_alloc_ts: low_ts,
            upper_fin_ts: upper_ts,
            running,
        }
    }

    fn xid(&self) -> TimeSeq {
        self.creator_ts
    }

    fn is_visible(&self, xid: TimeSeq) -> bool {
        // when the snapshot created
        if xid == self.creator_ts {
            // xid is the current transaction
            true
        } else if xid < self.upper_fin_ts {
            // xid has been committed
            true
        } else if xid > self.lower_un_alloc_ts {
            // xid has not been committed
            false
        } else if self.running.contains(&xid) {
            // xid is running
            false
        } else {
            // xid has been committed
            true
        }
    }

    fn is_tuple_visible(&self, tuple_ts: &Timestamp) -> bool {
        !self.is_visible(tuple_ts.c_max()) && self.is_visible(tuple_ts.c_min())
    }
}
