#![allow(clippy::unwrap_used)]
use crate::command::create_table::CreateTable;
use crate::contract::cmd_exec::CmdExec;

use crate::contract::meta_mgr::MetaMgr;
use crate::contract::schema_column::SchemaColumn;
use crate::contract::schema_table::SchemaTable;
use crate::contract::table_desc::TableDesc;
use crate::contract::table_info::TableInfo;
use crate::server::worker_snapshot::WorkerSnapshot;
use crate::wal::xl_batch::XLBatch;
use crate::x_engine::api::XContract;
use crate::x_engine::tx_mgr::{PhysicalRelationId, TxMgr};
use crate::x_engine::x_param::PCreateTable;
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

fn sample_schema(name: &str) -> SchemaTable {
    SchemaTable::new(
        name.to_string(),
        vec![SchemaColumn::new(
            "k".to_string(),
            TypeFamily::I64,
            DataType::new_no_param(TypeFamily::I64).to_info(),
        )],
        vec![0],
        vec![],
    )
}

fn dummy_table_desc(name: &str) -> Arc<TableDesc> {
    TableInfo::new(sample_schema(name))
        .unwrap()
        .table_desc()
        .unwrap()
}

fn make_param(schema: SchemaTable) -> PCreateTable {
    PCreateTable {
        tx_mgr: Arc::new(MockTxMgr),
        schema,
        partition_binding: None,
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
    create_table_called: AtomicBool,
}

impl MockXContract {
    fn new() -> Self {
        Self {
            create_table_called: AtomicBool::new(false),
        }
    }
}

#[async_trait]
impl XContract for MockXContract {
    async fn create_table(&self, _tx_mgr: Arc<dyn TxMgr>, _schema: &SchemaTable) -> RS<()> {
        self.create_table_called.store(true, Ordering::Relaxed);
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
        _pred_key: &crate::x_engine::api::VecDatum,
        _pred_non_key: &crate::x_engine::api::Predicate,
        _values: &crate::x_engine::api::VecDatum,
        _opt_update: &crate::x_engine::api::OptUpdate,
    ) -> RS<usize> {
        Ok(0)
    }
    async fn read_key(
        &self,
        _tx_mgr: Arc<dyn TxMgr>,
        _table_id: OID,
        _pred_key: &crate::x_engine::api::VecDatum,
        _select: &crate::x_engine::api::VecSelTerm,
        _opt_read: &crate::x_engine::api::OptRead,
    ) -> RS<Option<Vec<Option<Buf>>>> {
        Ok(None)
    }
    async fn read_range(
        &self,
        _tx_mgr: Arc<dyn TxMgr>,
        _table_id: OID,
        _pred_key: &crate::x_engine::api::RangeData,
        _pred_non_key: &crate::x_engine::api::Predicate,
        _select: &crate::x_engine::api::VecSelTerm,
        _opt_read: &crate::x_engine::api::OptRead,
    ) -> RS<Arc<dyn crate::x_engine::api::RSCursor>> {
        Err(mudu::mudu_error!(
            mudu::error::ErrorCode::NotImplemented,
            "mock read_range"
        ))
    }
    async fn delete(
        &self,
        _tx_mgr: Arc<dyn TxMgr>,
        _table_id: OID,
        _pred_key: &crate::x_engine::api::VecDatum,
        _pred_non_key: &crate::x_engine::api::Predicate,
        _opt_delete: &crate::x_engine::api::OptDelete,
    ) -> RS<usize> {
        Ok(0)
    }
    async fn insert(
        &self,
        _tx_mgr: Arc<dyn TxMgr>,
        _table_id: OID,
        _keys: &crate::x_engine::api::VecDatum,
        _values: &crate::x_engine::api::VecDatum,
        _opt_insert: &crate::x_engine::api::OptInsert,
    ) -> RS<()> {
        Ok(())
    }
}

struct MockMetaMgr {
    tables_by_name: SMutex<std::collections::HashMap<String, Arc<TableDesc>>>,
}

impl MockMetaMgr {
    fn new() -> Self {
        Self {
            tables_by_name: SMutex::new(std::collections::HashMap::new()),
        }
    }
    fn add_table(&self, name: &str) {
        self.tables_by_name
            .lock()
            .unwrap()
            .insert(name.to_string(), dummy_table_desc(name));
    }
}

#[async_trait]
impl MetaMgr for MockMetaMgr {
    async fn initialize(&self) -> RS<()> {
        Ok(())
    }
    async fn get_table_by_id(&self, oid: OID) -> RS<Arc<TableDesc>> {
        Err(mudu::mudu_error!(
            mudu::error::ErrorCode::EntityNotFound,
            format!("no such table {}", oid)
        ))
    }
    async fn get_table_by_name(&self, name: &str) -> RS<Option<Arc<TableDesc>>> {
        Ok(self.tables_by_name.lock().unwrap().get(name).cloned())
    }
    async fn create_table(&self, _schema: &SchemaTable) -> RS<()> {
        Ok(())
    }
    async fn drop_table(&self, _table_id: OID) -> RS<()> {
        Ok(())
    }
}

#[test]
fn prepare_succeeds_when_table_does_not_exist() {
    let cmd = CreateTable::new(
        make_param(sample_schema("fresh_t")),
        Arc::new(MockXContract::new()),
        Arc::new(MockMetaMgr::new()),
    );
    block_on(async { cmd.prepare().await }).unwrap();
}

#[test]
fn prepare_fails_when_table_already_exists() {
    let meta = Arc::new(MockMetaMgr::new());
    meta.add_table("existing_t");
    let cmd = CreateTable::new(
        make_param(sample_schema("existing_t")),
        Arc::new(MockXContract::new()),
        meta,
    );
    let err = block_on(async { cmd.prepare().await }).unwrap_err();
    assert_eq!(err.ec(), mudu::error::ErrorCode::EntityAlreadyExists);
}
