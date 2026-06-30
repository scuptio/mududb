#![allow(clippy::unwrap_used)]
use crate::command::insert_key_value::InsertKeyValue;
use crate::contract::cmd_exec::CmdExec;

use crate::contract::meta_mgr::MetaMgr;
use crate::contract::schema_column::SchemaColumn;
use crate::contract::schema_table::SchemaTable;
use crate::contract::table_desc::TableDesc;
use crate::contract::table_info::TableInfo;
use crate::server::worker_snapshot::WorkerSnapshot;
use crate::wal::xl_batch::XLBatch;
use crate::x_engine::api::{
    OptDelete, OptInsert, OptRead, OptUpdate, Predicate, RSCursor, RangeData, VecDatum, VecSelTerm,
    XContract,
};
use crate::x_engine::tx_mgr::{PhysicalRelationId, TxMgr};
use crate::x_engine::x_param::PInsertKeyValue;
use async_trait::async_trait;
use mudu::common::buf::Buf;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu_type::dat_type::DatType;
use mudu_type::dat_type_id::DatTypeID;
use std::collections::BTreeMap;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};

fn block_on<F>(fut: F) -> F::Output
where
    F: std::future::Future,
{
    mudu_sys::task::async_::build_current_thread_runtime()
        .unwrap()
        .block_on(fut)
}

fn dummy_table_desc() -> Arc<TableDesc> {
    let schema = SchemaTable::new(
        "t".to_string(),
        vec![SchemaColumn::new(
            "k".to_string(),
            DatTypeID::I64,
            DatType::new_no_param(DatTypeID::I64).to_info(),
        )],
        vec![0],
        vec![],
    );
    TableInfo::new(schema).unwrap().table_desc().unwrap()
}

fn datum(v: i64) -> Buf {
    v.to_be_bytes().to_vec()
}

fn make_param(rows: Vec<(VecDatum, VecDatum)>) -> PInsertKeyValue {
    PInsertKeyValue {
        tx_mgr: Arc::new(MockTxMgr),
        table_id: 1,
        rows,
    }
}

struct MockTxMgr;

impl TxMgr for MockTxMgr {
    fn xid(&self) -> u64 {
        1
    }
    fn snapshot(&self) -> WorkerSnapshot {
        WorkerSnapshot::new(1, Vec::new())
    }
    fn put(&self, _key: Vec<u8>, _value: Vec<u8>) {}
    fn delete(&self, _key: Vec<u8>) {}
    fn get(&self, _key: &[u8]) -> Option<Option<Vec<u8>>> {
        None
    }
    fn put_relation(&self, _relation_id: PhysicalRelationId, _key: Vec<u8>, _value: Vec<u8>) {}
    fn delete_relation(&self, _relation_id: PhysicalRelationId, _key: Vec<u8>) {}
    fn get_relation(
        &self,
        _relation_id: PhysicalRelationId,
        _key: &[u8],
    ) -> Option<Option<Vec<u8>>> {
        None
    }
    fn staged_relation_items_in_range(
        &self,
        _relation_id: PhysicalRelationId,
        _start_key: &[u8],
        _end_key: &[u8],
    ) -> Vec<(Vec<u8>, Option<Vec<u8>>)> {
        Vec::new()
    }
    fn staged_relation_ops(
        &self,
    ) -> BTreeMap<PhysicalRelationId, BTreeMap<Vec<u8>, Option<Vec<u8>>>> {
        BTreeMap::new()
    }
    fn staged_items_in_range(
        &self,
        _start_key: &[u8],
        _end_key: &[u8],
    ) -> Vec<(Vec<u8>, Option<Vec<u8>>)> {
        Vec::new()
    }
    fn staged_put_items(&self) -> BTreeMap<Vec<u8>, Option<Vec<u8>>> {
        BTreeMap::new()
    }
    fn is_empty(&self) -> bool {
        true
    }
    fn write_ops(&self) -> Vec<(PhysicalRelationId, Vec<u8>)> {
        Vec::new()
    }
    fn build_write_ops(&self) {}
    fn xl_batch(&self) -> XLBatch {
        XLBatch::new(Vec::new())
    }
}

struct MockXContract {
    insert_count: AtomicU64,
}

impl MockXContract {
    fn new() -> Self {
        Self {
            insert_count: AtomicU64::new(0),
        }
    }
}

#[async_trait]
impl XContract for MockXContract {
    async fn create_table(&self, _tx_mgr: Arc<dyn TxMgr>, _schema: &SchemaTable) -> RS<()> {
        Ok(())
    }
    async fn drop_table(&self, _tx_mgr: Arc<dyn TxMgr>, _oid: OID) -> RS<()> {
        Ok(())
    }
    async fn alter_table(
        &self,
        _tx_mgr: Arc<dyn TxMgr>,
        _oid: OID,
        _alter_table: &crate::x_engine::api::AlterTable,
    ) -> RS<()> {
        Ok(())
    }
    async fn begin_tx(&self) -> RS<Arc<dyn TxMgr>> {
        Ok(Arc::new(MockTxMgr))
    }
    async fn commit_tx(&self, _tx_mgr: Arc<dyn TxMgr>) -> RS<()> {
        Ok(())
    }
    async fn abort_tx(&self, _tx_mgr: Arc<dyn TxMgr>) -> RS<()> {
        Ok(())
    }
    async fn update(
        &self,
        _tx_mgr: Arc<dyn TxMgr>,
        _table_id: OID,
        _pred_key: &VecDatum,
        _pred_non_key: &Predicate,
        _values: &VecDatum,
        _opt_update: &OptUpdate,
    ) -> RS<usize> {
        Ok(0)
    }
    async fn read_key(
        &self,
        _tx_mgr: Arc<dyn TxMgr>,
        _table_id: OID,
        _pred_key: &VecDatum,
        _select: &VecSelTerm,
        _opt_read: &OptRead,
    ) -> RS<Option<Vec<Option<Buf>>>> {
        Ok(None)
    }
    async fn read_range(
        &self,
        _tx_mgr: Arc<dyn TxMgr>,
        _table_id: OID,
        _pred_key: &RangeData,
        _pred_non_key: &Predicate,
        _select: &VecSelTerm,
        _opt_read: &OptRead,
    ) -> RS<Arc<dyn RSCursor>> {
        Err(mudu::mudu_error!(
            mudu::error::ErrorCode::NotImplemented,
            "mock read_range"
        ))
    }
    async fn delete(
        &self,
        _tx_mgr: Arc<dyn TxMgr>,
        _table_id: OID,
        _pred_key: &VecDatum,
        _pred_non_key: &Predicate,
        _opt_delete: &OptDelete,
    ) -> RS<usize> {
        Ok(0)
    }
    async fn insert(
        &self,
        _tx_mgr: Arc<dyn TxMgr>,
        _table_id: OID,
        _keys: &VecDatum,
        _values: &VecDatum,
        _opt_insert: &OptInsert,
    ) -> RS<()> {
        self.insert_count.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }
}

struct MockMetaMgr;

#[async_trait]
impl MetaMgr for MockMetaMgr {
    async fn initialize(&self) -> RS<()> {
        Ok(())
    }
    async fn get_table_by_id(&self, _oid: OID) -> RS<Arc<TableDesc>> {
        Ok(dummy_table_desc())
    }
    async fn get_table_by_name(&self, _name: &str) -> RS<Option<Arc<TableDesc>>> {
        Ok(None)
    }
    async fn create_table(&self, _schema: &SchemaTable) -> RS<()> {
        Ok(())
    }
    async fn drop_table(&self, _table_id: OID) -> RS<()> {
        Ok(())
    }
}

#[test]
fn prepare_fails_for_empty_key() {
    let cmd = InsertKeyValue::new(
        make_param(vec![(
            VecDatum::new(Vec::new()),
            VecDatum::new(vec![(0, datum(1))]),
        )]),
        Arc::new(MockXContract::new()),
        Arc::new(MockMetaMgr),
    );
    let err = block_on(async { cmd.prepare().await }).unwrap_err();
    assert_eq!(err.ec(), mudu::error::ErrorCode::EntityNotFound);
}

#[test]
fn prepare_succeeds_for_non_empty_key() {
    let cmd = InsertKeyValue::new(
        make_param(vec![(
            VecDatum::new(vec![(0, datum(1))]),
            VecDatum::new(vec![(0, datum(2))]),
        )]),
        Arc::new(MockXContract::new()),
        Arc::new(MockMetaMgr),
    );
    block_on(async { cmd.prepare().await }).unwrap();
}

#[test]
fn affected_rows_matches_inserted_row_count() {
    let x_contract = Arc::new(MockXContract::new());
    let cmd = InsertKeyValue::new(
        make_param(vec![
            (
                VecDatum::new(vec![(0, datum(1))]),
                VecDatum::new(vec![(0, datum(10))]),
            ),
            (
                VecDatum::new(vec![(0, datum(2))]),
                VecDatum::new(vec![(0, datum(20))]),
            ),
            (
                VecDatum::new(vec![(0, datum(3))]),
                VecDatum::new(vec![(0, datum(30))]),
            ),
        ]),
        x_contract.clone(),
        Arc::new(MockMetaMgr),
    );
    block_on(async {
        cmd.prepare().await.unwrap();
        cmd.run().await.unwrap();
        assert_eq!(cmd.affected_rows().await.unwrap(), 3);
    });
    assert_eq!(x_contract.insert_count.load(Ordering::Relaxed), 3);
}
