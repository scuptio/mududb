use crate::contract::mem_store::MemStore;
use crate::contract::meta_mgr::MetaMgr;
use crate::contract::x_lock_mgr::{LockResult, XLockMgr};
use crate::contract::x_log::XLog;
use crate::tx::x_snap_mgr::SnapshotRequester;
use async_trait::async_trait;
use std::cell::RefCell;
use std::cmp::Ordering;
use std::sync::Arc;

use crate::contract::data_row::DataRow;
use crate::contract::schema_table::SchemaTable;
use crate::contract::table_desc::TableDesc;
use crate::storage::pst_op_ch::PstOpCh;
use crate::sync::notify_wait::create_notify_wait;
use crate::tx::tx_ctx::TxCtx;
use crate::x_engine::api::{
    AlterTable,
    OptDelete,
    OptInsert,
    OptRead,
    OptUpdate,
    Predicate,
    RSCursor,
    RangeData,
    VecDatum,
    VecSelTerm,
    XContract,
};
use mudu::common::buf::Buf;
use mudu::common::id::{ThdID, OID};
use mudu::common::result::RS;
use mudu::common::result_of::rs_of_opt;
use mudu::common::update_delta::UpdateDelta;
use mudu::common::xid::XID;
use mudu::error::ec::EC as ER;
use mudu::m_error;
use mudu::tuple::build_tuple::build_tuple;
use mudu::tuple::tuple_binary::TupleBinary as TupleRaw;
use mudu::tuple::update_tuple::update_tuple;
use mudu_utils::task_trace;
use scc::HashMap;

#[derive(Clone)]
pub struct ThdCtx {
    inner: Arc<ThdCtxInner>,
}

struct ThdCtxInner {
    id: u64,
    meta_mgr: Arc<dyn MetaMgr>,
    snap_req: Arc<SnapshotRequester>,
    x_lock_mgr: Arc<dyn XLockMgr>,
    x_log: Vec<Arc<dyn XLog>>,
    tree_store: Arc<dyn MemStore>,
    pst_op_ch: Arc<dyn PstOpCh>,
    tx_ctx: HashMap<XID, TxCtx>,
}

impl ThdCtx {
    pub fn new(
        id: u64,
        meta_mgr: Arc<dyn MetaMgr>,
        snap_req: Arc<SnapshotRequester>,
        x_lock_mgr: Arc<dyn XLockMgr>,
        x_log: Vec<Arc<dyn XLog>>,
        tree_store: Arc<dyn MemStore>,
        pst_op_ch: Arc<dyn PstOpCh>,
    ) -> Self {
        Self {
            inner: Arc::new(ThdCtxInner::new(
                id, meta_mgr, snap_req, x_lock_mgr, x_log, tree_store, pst_op_ch,
            )),
        }
    }

    pub fn thd_id(&self) -> ThdID {
        self.inner.id
    }

    pub fn snap_req(&self) -> &SnapshotRequester {
        self.inner.snap_req()
    }

    pub fn x_log(&self) -> &Vec<Arc<dyn XLog>> {
        self.inner.x_log()
    }

    pub fn meta_mgr(&self) -> &dyn MetaMgr {
        self.inner.meta_mgr()
    }

    pub fn tree_store(&self) -> &dyn MemStore {
        self.inner.tree_store()
    }

    pub fn x_lock_mgr(&self) -> &dyn XLockMgr {
        self.inner.x_lock_mgr()
    }

    pub fn pst_op_ch(&self) -> &dyn PstOpCh {
        self.inner.pst_op_ch()
    }
}

impl ThdCtxInner {
    fn new(
        id: u64,
        meta_mgr: Arc<dyn MetaMgr>,
        snap_req: Arc<SnapshotRequester>,
        x_lock_mgr: Arc<dyn XLockMgr>,
        x_log: Vec<Arc<dyn XLog>>,
        tree_store: Arc<dyn MemStore>,
        pst_op_ch: Arc<dyn PstOpCh>,
    ) -> Self {
        Self {
            id,
            snap_req,
            meta_mgr,
            x_log,
            tree_store,
            x_lock_mgr,
            pst_op_ch,
            tx_ctx: Default::default(),
        }
    }

    fn snap_req(&self) -> &SnapshotRequester {
        self.snap_req.as_ref()
    }

    fn x_log(&self) -> &Vec<Arc<dyn XLog>> {
        &self.x_log
    }

    fn meta_mgr(&self) -> &dyn MetaMgr {
        self.meta_mgr.as_ref()
    }

    fn tree_store(&self) -> &dyn MemStore {
        self.tree_store.as_ref()
    }

    fn x_lock_mgr(&self) -> &dyn XLockMgr {
        self.x_lock_mgr.as_ref()
    }
    fn pst_op_ch(&self) -> &dyn PstOpCh {
        self.pst_op_ch.as_ref()
    }

    async fn create_table(&self, _xid: XID, schema: &SchemaTable) -> RS<()> {
        task_trace!();
        let table_id = schema.id();
        self.meta_mgr.create_table(schema).await?;
        let kv_desc = self.meta_mgr.get_table_by_id(table_id).await?;
        self.x_lock_mgr
            .create_table(table_id, kv_desc.key_desc().clone())
            .await?;
        self.tree_store
            .create_table(table_id, kv_desc.key_desc().clone())
            .await?;
        Ok(())
    }

    async fn get_desc(&self, table_id: OID) -> RS<Arc<TableDesc>> {
        self.meta_mgr().get_table_by_id(table_id).await
    }

    fn pk_build_tuple(pkey: &VecDatum, desc: &TableDesc) -> RS<Buf> {
        Self::_build_tuple::<true>(pkey.data(), desc)
    }

    fn val_build_tuple(val: &VecDatum, desc: &TableDesc) -> RS<Buf> {
        Self::_build_tuple::<false>(val.data(), desc)
    }

    fn val_update_tuple(
        tuple: &TupleRaw,
        val: &VecDatum,
        desc: &TableDesc,
    ) -> RS<Vec<UpdateDelta>> {
        Self::_update_tuple(tuple, val.data(), desc)
    }

    // build update tuple for this row
    fn _update_tuple(
        tuple: &TupleRaw,
        datum: &Vec<(OID, Buf)>,
        table_desc: &TableDesc,
    ) -> RS<Vec<UpdateDelta>> {
        let mut delta = vec![];
        for (id, dat) in datum.iter() {
            let opt = table_desc.oid2col().get(id);
            let field = rs_of_opt(opt, || {
                m_error!(ER::IOErr, format!("no column {} in table {}", id, table_desc.id()))
            })?;
            let datum_index = field.datum_index();
            update_tuple(datum_index, dat, table_desc.value_desc(), tuple, &mut delta)?;
        }
        Ok(delta)
    }

    fn _build_tuple<const IS_KEY: bool>(data: &Vec<(OID, Buf)>, desc: &TableDesc) -> RS<Buf> {
        let mut vec_data = data.clone();
        let ok = RefCell::new(true);
        vec_data.sort_by(|(id1, _), (id2, _)| {
            let (opt1, opt2) = (desc.oid2col().get(id1), desc.oid2col().get(id2));
            let mut b_ok = ok.borrow_mut();
            match (opt1, opt2) {
                (Some(i1), Some(i2)) => {
                    if i1.is_primary() != i2.is_primary() {
                        *b_ok = false;
                    }
                    i1.datum_index().cmp(&i2.datum_index())
                }
                (None, None) => {
                    *b_ok = false;
                    Ordering::Equal
                }
                (Some(_), None) => {
                    *b_ok = false;
                    Ordering::Greater
                }
                (None, Some(_)) => {
                    *b_ok = false;
                    Ordering::Less
                }
            }
        });
        if !*ok.borrow() {
            return Err(m_error!(ER::TupleErr));
        }
        let vec_data: Vec<_> = vec_data.into_iter().map(|(_, v)| v).collect();
        let desc = if IS_KEY {
            desc.key_desc()
        } else {
            desc.value_desc()
        };
        if desc.field_count() != vec_data.len() {
            return Err(m_error!(ER::TupleErr));
        }
        let tuple = build_tuple(&vec_data, desc)?;
        Ok(tuple)
    }

    async fn lock_x(&self, tx_ctx: &TxCtx, table_id: OID, key: Buf) -> RS<()> {
        task_trace!();
        let xid = tx_ctx.xid();
        tx_ctx.write(table_id, key.clone()).await?;
        let (notify, wait) = create_notify_wait();
        self.x_lock_mgr
            .lock(notify, xid, table_id, key.clone())
            .await?;
        let opt = wait.wait().await?;
        match opt {
            Some(lock_r) => match lock_r {
                LockResult::Locked => Ok(()),
                LockResult::LockFailed => {
                    Err(m_error!(ER::TxErr, format!("transaction {} lock failed", xid)))
                }
            },
            None => Err(m_error!(ER::TxErr, format!(
                "transaction {} lock failed",
                tx_ctx.xid()
            ))),
        }
    }

    async fn insert(
        &self,
        xid: XID,
        table_id: OID,
        keys: &VecDatum,
        values: &VecDatum,
        _opt_insert: &OptInsert,
    ) -> RS<()> {
        task_trace!();
        let tx_ctx = self.get_tx_ctx(xid)?;
        let (key, value) = {
            let desc = self.get_desc(table_id).await?;
            let key = Self::pk_build_tuple(keys, &desc)?;
            let value = Self::val_build_tuple(values, &desc)?;
            (key, value)
        };
        self.lock_x(&tx_ctx, table_id, key.clone()).await?;
        let opt = self.tree_store.get_key(table_id, key.clone()).await?;
        if opt.is_some() {
            return Err(m_error!(ER::ExistingSuchElement, format!(
                "existing key for table {}",
                table_id)));
        }
        let data_row = DataRow::new();
        tx_ctx.insert(table_id, key, value, data_row).await?;
        Ok(())
    }

    async fn update(
        &self,
        xid: XID,
        table_id: OID,
        pred_key: &VecDatum,
        _pred_non_key: &Predicate,
        values: &VecDatum,
        _opt_update: &OptUpdate,
    ) -> RS<usize> {
        let tx_ctx = self.get_tx_ctx(xid)?;
        let desc = self.get_desc(table_id).await?;
        let key = { Self::pk_build_tuple(pred_key, &desc)? };
        let opt = self.tree_store.get_key(table_id, key.clone()).await?;
        let data_row = match opt {
            Some(row) => row,
            None => {
                return Err(m_error!(ER::NoSuchElement, format!(
                    "no existing key for table {} update",
                    table_id
                )));
            }
        };
        let opt_tuple_id = data_row.tuple_id().await?;
        let tuple_id = rs_of_opt(opt_tuple_id, || {
            m_error!(ER::NoSuchElement, format!("no existing key for table {} update", table_id))
        })?;
        let opt_tuple_version = data_row.read_latest().await?;
        let tuple_version = rs_of_opt(opt_tuple_version, || {
            m_error!(ER::NoSuchElement, format!("no existing key for table {} update", table_id))
        })?;
        let tuple = tuple_version.tuple();
        let vec_delta = Self::val_update_tuple(tuple, values, &desc)?;
        tx_ctx
            .update(table_id, tuple_id, key, vec_delta, data_row)
            .await?;
        Ok(1)
    }

    async fn read_key(
        &self,
        xid: XID,
        table_id: OID,
        pred_key: &VecDatum,
        select: &VecSelTerm,
        _opt_read: &OptRead,
    ) -> RS<Option<Vec<Buf>>> {
        let _tx_ctx = self.get_tx_ctx(xid)?;
        let desc = self.get_desc(table_id).await?;
        let key = { Self::pk_build_tuple(pred_key, &desc)? };
        let opt = self.tree_store.get_key(table_id, key.clone()).await?;
        let data_row = match opt {
            Some(row) => row,
            None => {
                return Ok(None);
            }
        };
        let opt_row = data_row.read_latest().await?;
        let tuple = match &opt_row {
            Some(version) => version.tuple(),
            None => {
                return Ok(None);
            }
        };
        let mut tuple_ret = vec![];
        for i in select.vec() {
            let opt = desc.oid2col().get(i);
            if let Some(f) = opt {
                let index = f.datum_index();
                let desc = if f.is_primary() {
                    desc.key_desc().get_field_desc(index)
                } else {
                    desc.value_desc().get_field_desc(index)
                };
                let slice = desc.get(tuple)?;
                tuple_ret.push(slice.to_vec());
            }
        }
        Ok(Some(tuple_ret))
    }

    async fn _begin_tx(&self) -> RS<XID> {
        task_trace!();
        let snapshot = self.snap_req.start_tx().await?;
        let xid = snapshot.xid();
        let tx_ctx = TxCtx::new(xid, snapshot);
        let _ = self.tx_ctx.insert(xid, tx_ctx);
        Ok(xid)
    }

    async fn _commit_tx(&self, xid: XID) -> RS<()> {
        task_trace!();
        let tx_ctx = self.get_tx_ctx(xid)?;
        tx_ctx.commit(&*self.x_lock_mgr).await?;
        self.snap_req.end_tx(xid).await?;
        self.remove_tx_ctx(xid);
        Ok(())
    }

    fn get_tx_ctx(&self, xid: XID) -> RS<TxCtx> {
        let opt = self.tx_ctx.get(&xid);
        let entry = rs_of_opt(opt, || {
            m_error!(ER::NoSuchElement, format!("no such transaction {}", xid))
        })?;
        let ctx = entry.get().clone();
        Ok(ctx)
    }

    fn remove_tx_ctx(&self, xid: XID) {
        self.tx_ctx.remove(&xid);
    }
}

#[async_trait]
impl XContract for ThdCtx {
    async fn create_table(&self, xid: XID, schema: &SchemaTable) -> RS<()> {
        task_trace!();
        self.inner.create_table(xid, schema).await
    }

    async fn drop_table(&self, _xid: XID, _oid: OID) -> RS<()> {
        todo!()
    }

    async fn alter_table(&self, _xid: XID, _oid: OID, _alter_table: &AlterTable) -> RS<()> {
        todo!()
    }

    async fn begin_tx(&self) -> RS<XID> {
        self.inner._begin_tx().await
    }

    async fn commit_tx(&self, xid: XID) -> RS<()> {
        self.inner._commit_tx(xid).await
    }

    async fn abort_tx(&self, _xid: XID) -> RS<()> {
        todo!()
    }

    async fn update(
        &self,
        xid: XID,
        table_id: OID,
        pred_key: &VecDatum,
        pred_non_key: &Predicate,
        values: &VecDatum,
        opt_update: &OptUpdate,
    ) -> RS<usize> {
        self.inner
            .update(xid, table_id, pred_key, pred_non_key, values, opt_update)
            .await
    }

    async fn read_key(
        &self,
        xid: XID,
        table_id: OID,
        pred_key: &VecDatum,
        vec_proj: &VecSelTerm,
        opt_read: &OptRead,
    ) -> RS<Option<Vec<Buf>>> {
        self.inner
            .read_key(xid, table_id, pred_key, vec_proj, opt_read)
            .await
    }

    async fn read_range(
        &self,
        _xid: XID,
        _table_id: OID,
        _pred_key: &RangeData,
        _pred_non_key: &Predicate,
        _select: &VecSelTerm,
        _opt_read: &OptRead,
    ) -> RS<Arc<dyn RSCursor>> {
        todo!()
    }

    async fn delete(
        &self,
        _xid: XID,
        _table_id: OID,
        _pred_key: &VecDatum,
        _pred_non_key: &Predicate,
        _opt_delete: &OptDelete,
    ) -> RS<usize> {
        todo!()
    }

    async fn insert(
        &self,
        xid: XID,
        table_id: OID,
        keys: &VecDatum,
        values: &VecDatum,
        opt_insert: &OptInsert,
    ) -> RS<()> {
        task_trace!();
        self.inner
            .insert(xid, table_id, keys, values, opt_insert)
            .await
    }
}
