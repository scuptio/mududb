use crate::contract::x_lock_mgr::LockResult;
use crate::sync::notify_wait::Notify;
use crate::sync::s_mutex::SMutex;
use mudu::common::result::RS;
use mudu::common::xid::XID;
use std::collections::VecDeque;
use std::sync::Arc;

#[derive(Clone)]
pub struct LockSlot {
    inner: Arc<SMutex<_LockSlotInner>>,
}

pub struct _LockSlotInner {
    is_deleted: bool,
    locked: Option<XID>,
    queue: VecDeque<(XID, Notify<LockResult>)>,
}

impl LockSlot {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(SMutex::new(_LockSlotInner::new())),
        }
    }

    /// when return None, this has been deleted, the invoker mut recreate a new LockSlot
    pub fn lock(&self, xid: XID, notify: Notify<LockResult>) -> Option<RS<()>> {
        let r = self.inner.lock();
        let mut g = match r {
            Ok(g) => g,
            Err(e) => return Some(Err(e)),
        };
        if g.is_deleted() {
            None
        } else {
            let r = g.lock(xid, notify);
            match r {
                Ok(_) => Some(Ok(())),
                Err(e) => Some(Err(e)),
            }
        }
    }

    pub fn release(&self, xid: XID) -> RS<bool> {
        let mut g = self.inner.lock()?;
        let (_, opt_notify) = g.release(xid);
        if let Some((_, notify)) = opt_notify {
            notify.notify(LockResult::Locked)?;
        }
        Ok(g.remove_if_empty())
    }
}

impl _LockSlotInner {
    fn new() -> Self {
        Self {
            is_deleted: false,
            locked: None,
            queue: Default::default(),
        }
    }

    fn is_deleted(&self) -> bool {
        self.is_deleted
    }

    fn set_deleted(&mut self) {
        self.is_deleted = true;
    }

    fn lock(&mut self, xid: XID, notify: Notify<LockResult>) -> RS<bool> {
        let ret = if self.locked.is_none() {
            assert!(self.queue.is_empty());
            self.locked = Some(xid);
            notify.notify(LockResult::Locked)?;
            true
        } else {
            self.queue.push_back((xid, notify));
            false
        };
        Ok(ret)
    }

    fn release(&mut self, xid: XID) -> (bool, Option<(XID, Notify<LockResult>)>) {
        let ok = self.clear_locked(xid);
        if ok {
            let notify = self.notify_next();
            (true, notify)
        } else {
            (false, None)
        }
    }

    fn clear_locked(&mut self, xid: XID) -> bool {
        match self.locked {
            Some(id) => {
                if xid == id {
                    self.locked = None;
                    true
                } else {
                    false
                }
            }
            None => false,
        }
    }

    fn notify_next(&mut self) -> Option<(XID, Notify<LockResult>)> {
        if let Some(_t) = self.locked {
            return None;
        }
        let opt = self.queue.pop_front();
        match opt {
            Some((xid, notify)) => {
                self.locked = Some(xid);
                Some((xid, notify))
            }
            None => None,
        }
    }

    fn remove_if_empty(&mut self) -> bool {
        if self.locked.is_none() && self.queue.is_empty() {
            self.is_deleted = true;
        }
        self.is_deleted
    }
}
