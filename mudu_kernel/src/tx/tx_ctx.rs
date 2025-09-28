use crate::contract::data_row::DataRow;
use crate::contract::pst_op_list::PstOpList;
use crate::contract::snapshot::{Snapshot, TimeSeq};
use crate::contract::timestamp::Timestamp;
use crate::contract::version_tuple::VersionTuple;
use crate::contract::x_lock_mgr::XLockMgr;
use crate::contract::xl_rec::XLRec;
use crate::sync::a_mutex::AMutex;
use mudu::common::buf::Buf;
use mudu::common::id::{gen_oid, OID};
use mudu::common::result::RS;
use mudu::common::update_delta::UpdateDelta;
use mudu::common::xid::XID;
use mudu_utils::task_trace;
use std::sync::Arc;

#[derive(Clone)]
pub struct TxCtx {
    xid: XID,
    inner: Arc<AMutex<_TxCtx>>,
}

struct _TxCtx {
    xid: XID,
    snapshot: Snapshot,
    write_key: Vec<(OID, Buf)>,
    log_rec: XLRec,
    ops: Vec<TxWriteOp>,
}

enum TxWriteOp {
    Insert(TxInsert),
    Update(TxUpdate),
    Delete,
}

struct TxInsert {
    table_id: OID,
    tuple_id: OID,
    key: Buf,
    value: Buf,
    row: DataRow,
}

struct TxUpdate {
    table_id: OID,
    tuple_id: OID,
    key: Buf,
    value: Buf,
    value_up: Vec<UpdateDelta>,
    row: DataRow,
}

impl TxCtx {
    pub fn new(xid: XID, snapshot: Snapshot) -> Self {
        Self {
            xid,
            inner: Arc::new(AMutex::new(_TxCtx::new(xid, snapshot))),
        }
    }

    pub fn xid(&self) -> XID {
        self.xid
    }

    pub async fn insert(&self, table_id: OID, keys: Buf, values: Buf, row: DataRow) -> RS<()> {
        task_trace!();
        let mut g = self.inner.lock().await;
        g.insert(table_id, keys, values, row).await?;
        Ok(())
    }

    pub async fn update(
        &self,
        table_id: OID,
        tuple_id: OID,
        keys: Buf,
        values: Vec<UpdateDelta>,
        row: DataRow,
    ) -> RS<()> {
        let mut g = self.inner.lock().await;
        g.update(table_id, tuple_id, keys, values, row);
        Ok(())
    }
    pub async fn write(&self, oid: OID, buf: Buf) -> RS<()> {
        let mut g = self.inner.lock().await;
        g.write(oid, buf);
        Ok(())
    }

    pub async fn commit(&self, lock_mgr: &dyn XLockMgr) -> RS<()> {
        task_trace!();
        let mut g = self.inner.lock().await;
        g.commit(lock_mgr).await?;
        Ok(())
    }

    async fn abort(&self, lock_mgr: &dyn XLockMgr) -> RS<()> {
        let mut g = self.inner.lock().await;
        g.abort(lock_mgr).await?;
        Ok(())
    }

    pub async fn snapshot(&self) -> RS<Snapshot> {
        let g = self.inner.lock().await;
        Ok(g.snapshot().clone())
    }
}

impl _TxCtx {
    pub fn new(xid: XID, snapshot: Snapshot) -> Self {
        Self {
            snapshot,
            xid,
            write_key: vec![],
            log_rec: XLRec::new(xid),
            ops: vec![],
        }
    }

    fn write(&mut self, oid: OID, key: Buf) {
        self.add_write_key(oid, key);
    }

    async fn commit(&mut self, lock_mgr: &dyn XLockMgr) -> RS<()> {
        task_trace!();
        self.clear(lock_mgr).await?;
        Ok(())
    }

    async fn abort(&mut self, lock_mgr: &dyn XLockMgr) -> RS<()> {
        task_trace!();
        self.clear(lock_mgr).await?;
        Ok(())
    }

    async fn clear(&self, lock_mgr: &dyn XLockMgr) -> RS<()> {
        task_trace!();
        for (id, key) in self.write_key.iter() {
            lock_mgr.release(self.xid, *id, key).await?;
        }
        Ok(())
    }

    fn add_write_key(&mut self, table: OID, key: Buf) {
        self.write_key.push((table, key));
    }

    fn snapshot(&self) -> &Snapshot {
        &self.snapshot
    }

    async fn insert(&mut self, table_id: OID, key: Buf, value: Buf, row: DataRow) -> RS<()> {
        task_trace!();
        let op = TxInsert {
            table_id,
            tuple_id: gen_oid(),
            key,
            value,
            row,
        };
        self.ops.push(TxWriteOp::Insert(op));
        Ok(())
    }

    fn update(
        &mut self,
        table_id: OID,
        tuple_id: OID,
        key: Buf,
        value_up: Vec<UpdateDelta>,
        row: DataRow,
    ) {
        let op = TxUpdate {
            table_id,
            tuple_id,
            key,
            value: vec![],
            value_up,
            row,
        };
        self.ops.push(TxWriteOp::Update(op));
    }

    async fn delete(&mut self, table_id: OID, tuple_id: OID, keys: Buf) {
        self.log_rec.add_delete(table_id, tuple_id, keys);
    }
}

trait XWriteOp {
    fn to_x_log_rec(&self, rec: &mut XLRec);

    async fn apply_to_mem(&self, xid: TimeSeq, row: &DataRow) -> RS<()>;

    fn apply_to_pst(&self, pst_op: PstOpList);
}

impl XWriteOp for TxUpdate {
    fn to_x_log_rec(&self, rec: &mut XLRec) {
        rec.add_update(
            self.table_id,
            self.tuple_id,
            self.key.clone(),
            self.value_up.clone(),
        );
    }

    async fn apply_to_mem(&self, xid: TimeSeq, row: &DataRow) -> RS<()> {
        let timestamp = Timestamp::new(xid, u64::MAX);

        row.write(VersionTuple::new(timestamp, self.value.clone()), None)
            .await?;
        Ok(())
    }

    fn apply_to_pst(&self, _pst_op: PstOpList) {
        todo!()
    }
}
