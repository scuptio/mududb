use crate::contract::snapshot::TimeSeq;
use crate::contract::x_lock_mgr::XLockMgr;
use crate::tx::x_lock_mgr::XLockMgrImpl;
use crate::tx::x_snap_mgr::XSnapMgr;
use mudu_utils::notifier::Notifier;
use std::sync::Arc;

pub struct TxMgrFactory {}

impl TxMgrFactory {
    pub fn create_lock_mgr() -> Arc<dyn XLockMgr> {
        Arc::new(XLockMgrImpl::new())
    }

    pub fn create_snap_mgr(
        canceller: Notifier,
        xid_max: TimeSeq,
        snap_request_queue_size: usize,
    ) -> Arc<XSnapMgr> {
        Arc::new(XSnapMgr::new(canceller, xid_max, snap_request_queue_size))
    }
}
