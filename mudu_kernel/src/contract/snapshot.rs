use crate::contract::timestamp::Timestamp;
use mudu::common::xid::XID;
use std::sync::Arc;

pub type TimeSeq = u64;

#[derive(Debug, Clone)]
pub struct RunningXList {
    creator_ts: u64,
    running: Vec<u64>,
}

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
    pub fn from(snapshot: RunningXList) -> Self {
        Self {
            inner: Arc::new(_Snapshot::from(snapshot)),
        }
    }

    pub fn xid(&self) -> XID {
        self.inner.xid() as _
    }

    pub fn is_visible(&self, xid: TimeSeq) -> bool {
        self.inner.is_visible(xid)
    }

    pub fn is_tuple_visible(&self, tuple_ts: &Timestamp) -> bool {
        self.inner.is_tuple_visible(tuple_ts)
    }
}

impl RunningXList {
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
