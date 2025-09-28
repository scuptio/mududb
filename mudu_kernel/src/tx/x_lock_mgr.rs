use crate::contract::x_lock_mgr::{LockResult, XLockMgr};
use crate::sync::notify_wait::Notify;
use crate::tx::lock_table::LockTable;
use async_trait::async_trait;
use mudu::common::buf::Buf;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::common::xid::XID;
use mudu::error::ec::EC as ER;
use mudu::m_error;
use mudu::tuple::tuple_binary_desc::TupleBinaryDesc as TupleDesc;
use scc::HashMap;
use std::sync::Arc;

#[derive(Clone)]
pub struct XLockMgrImpl {
    inner: Arc<_XLockMgrInner>,
}

struct _XLockMgrInner {
    map: HashMap<OID, LockTable>,
}

impl XLockMgrImpl {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(_XLockMgrInner {
                map: HashMap::new(),
            }),
        }
    }
}

impl _XLockMgrInner {
    fn new() -> Self {
        Self {
            map: HashMap::new(),
        }
    }

    fn create_table(&self, table: OID, tuple_desc: TupleDesc) -> RS<()> {
        let r = self.map.insert(table, LockTable::new(tuple_desc));
        if r.is_err() {
            return Err(m_error!(ER::ExistingSuchElement));
        }
        Ok(())
    }

    fn drop_table(&self, table: OID) -> RS<()> {
        let r = self.map.remove(&table);
        if r.is_none() {
            return Err(m_error!(ER::NoSuchElement));
        }
        Ok(())
    }
    fn lock(&self, notify: Notify<LockResult>, xid: XID, table_id: OID, key: Buf) -> RS<()> {
        let table = self.get_lock_table(table_id)?;
        table.lock(notify, xid, key)?;
        Ok(())
    }

    fn release(&self, xid: XID, table_id: OID, key: &Buf) -> RS<()> {
        let table = self.get_lock_table(table_id)?;
        table.release(xid, key)?;
        Ok(())
    }

    fn get_lock_table(&self, table_id: OID) -> RS<LockTable> {
        let lock_table = {
            let opt = self.map.get(&table_id);
            match opt {
                Some(e) => e.get().clone(),
                None => {
                    return Err(m_error!(ER::NoSuchElement, format!("no such table {:}", table_id)));
                }
            }
        };
        Ok(lock_table)
    }
}

#[async_trait]
impl XLockMgr for XLockMgrImpl {
    async fn create_table(&self, table: OID, tuple_desc: TupleDesc) -> RS<()> {
        self.inner.create_table(table, tuple_desc)
    }

    async fn drop_table(&self, table: OID) -> RS<()> {
        self.inner.drop_table(table)
    }

    async fn lock(&self, notify: Notify<LockResult>, xid: XID, table_id: OID, key: Buf) -> RS<()> {
        self.inner.lock(notify, xid, table_id, key)
    }

    async fn release(&self, xid: XID, table_id: OID, key: &Buf) -> RS<()> {
        self.inner.release(xid, table_id, key)
    }
}

unsafe impl Send for XLockMgrImpl {}
unsafe impl Sync for XLockMgrImpl {}
