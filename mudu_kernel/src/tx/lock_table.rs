use crate::collection::hash_map::hash_map_get_or_create;
use crate::contract::x_lock_mgr::LockResult;
use crate::sync::notify_wait::Notify;
use crate::tx::lock_slot::LockSlot;
use mudu::common::buf::Buf;
use mudu::common::result::RS;
use mudu::common::xid::XID;
use mudu::error::ec::EC as ER;
use mudu::m_error;
use mudu::tuple::tuple_binary_desc::TupleBinaryDesc as TupleDesc;
use mudu::tuple::tuple_key::TupleKey;
use scc::HashMap;
use std::sync::Arc;

#[derive(Clone)]
pub struct LockTable {
    inner: Arc<LockTableInner>,
}

struct LockTableInner {
    tuple_desc: TupleDesc,
    hash_map: HashMap<TupleKey, LockSlot>,
}

impl LockTable {
    pub fn new(tuple_desc: TupleDesc) -> Self {
        Self {
            inner: Arc::new(LockTableInner::new(tuple_desc)),
        }
    }

    pub fn lock(&self, notify: Notify<LockResult>, xid: XID, key: Buf) -> RS<()> {
        self.inner.lock(notify, xid, key)
    }

    pub fn release(&self, xid: XID, key: &Buf) -> RS<()> {
        self.inner.release(xid, key)
    }
}

impl LockTableInner {
    pub fn new(tuple_desc: TupleDesc) -> Self {
        Self {
            tuple_desc,
            hash_map: HashMap::new(),
        }
    }

    fn lock(&self, notify: Notify<LockResult>, xid: XID, key: Buf) -> RS<()> {
        let key = TupleKey::from_buf(&self.tuple_desc, key);

        hash_map_get_or_create(&self.hash_map, key, LockSlot::new, move |slot| {
            let n = notify.clone();
            slot.lock(xid, n)
        })?
    }

    fn release(&self, xid: XID, key: &Buf) -> RS<()> {
        let key = TupleKey::from_buf(&self.tuple_desc, key.clone());
        let opt = self.hash_map.get(&key);
        let slot = match opt {
            Some(slot) => slot.clone(),
            None => return Err(m_error!(ER::NoSuchElement, "")),
        };
        let empty = slot.release(xid)?;
        if empty {
            let _ = self.hash_map.remove(&key);
        }
        Ok(())
    }
}
