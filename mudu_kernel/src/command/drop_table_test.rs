#![allow(clippy::unwrap_used)]
use crate::command::drop_table::DropTable;
use crate::contract::cmd_exec::CmdExec;

use crate::contract::meta_mgr::MetaMgr;
use crate::contract::schema_column::SchemaColumn;
use crate::contract::schema_table::SchemaTable;
use crate::contract::table_desc::TableDesc;
use crate::contract::table_info::TableInfo;
use crate::server::worker_snapshot::WorkerSnapshot;
use crate::wal::xl_batch::XLBatch;
use crate::x_engine::api::{
    AlterTable, OptDelete, OptInsert, OptRead, OptUpdate, Predicate, RSCursor, RangeData, VecDatum,
    VecSelTerm, XContract,
};
use crate::x_engine::tx_mgr::{PhysicalRelationId, TxMgr};
use crate::x_engine::x_param::PDropTable;
use async_trait::async_trait;
use mudu::common::buf::Buf;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu_sys::sync::SMutex;
use mudu_type::data_type::DataType;
use mudu_type::type_family::TypeFamily;
use std::collections::BTreeMap;
use std::sync::{
    atomic::{AtomicBool, Ordering},
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
            TypeFamily::I64,
            DataType::new_no_param(TypeFamily::I64).to_info(),
        )],
        vec![0],
        vec![],
    );
    TableInfo::new(schema).unwrap().table_desc().unwrap()
}

fn make_param(oid: Option<OID>) -> PDropTable {
    PDropTable {
        tx_mgr: Arc::new(MockTxMgr),
        oid,
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
    drop_table_called: AtomicBool,
}

impl MockXContract {
    fn new() -> Self {
        Self {
            drop_table_called: AtomicBool::new(false),
        }
    }
}

#[async_trait]
impl XContract for MockXContract {
    async fn create_table(&self, _tx_mgr: Arc<dyn TxMgr>, _schema: &SchemaTable) -> RS<()> {
        Ok(())
    }
    async fn drop_table(&self, _tx_mgr: Arc<dyn TxMgr>, _oid: OID) -> RS<()> {
        self.drop_table_called.store(true, Ordering::Relaxed);
        Ok(())
    }
    async fn alter_table(
        &self,
        _tx_mgr: Arc<dyn TxMgr>,
        _oid: OID,
        _alter_table: &AlterTable,
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
        Ok(())
    }
}

struct MockMetaMgr {
    known_oids: SMutex<Vec<OID>>,
}

impl MockMetaMgr {
    fn new() -> Self {
        Self {
            known_oids: SMutex::new(Vec::new()),
        }
    }
    fn add_oid(&self, oid: OID) {
        self.known_oids.lock().unwrap().push(oid);
    }
}

#[async_trait]
impl MetaMgr for MockMetaMgr {
    async fn initialize(&self) -> RS<()> {
        Ok(())
    }
    async fn get_table_by_id(&self, oid: OID) -> RS<Arc<TableDesc>> {
        if self.known_oids.lock().unwrap().contains(&oid) {
            Ok(dummy_table_desc())
        } else {
            Err(mudu::mudu_error!(
                mudu::error::ErrorCode::EntityNotFound,
                format!("no such table {}", oid)
            ))
        }
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
fn prepare_succeeds_without_oid() {
    let cmd = DropTable::new(
        make_param(None),
        Arc::new(MockXContract::new()),
        Arc::new(MockMetaMgr::new()),
    );
    block_on(async { cmd.prepare().await }).unwrap();
}

#[test]
fn prepare_succeeds_with_known_oid() {
    let meta = Arc::new(MockMetaMgr::new());
    meta.add_oid(42);
    let cmd = DropTable::new(make_param(Some(42)), Arc::new(MockXContract::new()), meta);
    block_on(async { cmd.prepare().await }).unwrap();
}

#[test]
fn prepare_fails_with_unknown_oid() {
    let cmd = DropTable::new(
        make_param(Some(99)),
        Arc::new(MockXContract::new()),
        Arc::new(MockMetaMgr::new()),
    );
    let err = block_on(async { cmd.prepare().await }).unwrap_err();
    assert_eq!(err.ec(), mudu::error::ErrorCode::EntityNotFound);
}

#[test]
fn run_calls_drop_table_when_oid_present() {
    let x_contract = Arc::new(MockXContract::new());
    let cmd = DropTable::new(
        make_param(Some(7)),
        x_contract.clone(),
        Arc::new(MockMetaMgr::new()),
    );
    block_on(async { cmd.run().await }).unwrap();
    assert!(x_contract.drop_table_called.load(Ordering::Relaxed));
}

#[test]
fn run_skips_drop_table_when_oid_missing() {
    let x_contract = Arc::new(MockXContract::new());
    let cmd = DropTable::new(
        make_param(None),
        x_contract.clone(),
        Arc::new(MockMetaMgr::new()),
    );
    block_on(async { cmd.run().await }).unwrap();
    assert!(!x_contract.drop_table_called.load(Ordering::Relaxed));
}
