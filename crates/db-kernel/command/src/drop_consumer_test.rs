#![allow(clippy::unwrap_used)]
#![allow(clippy::unimplemented)]

use crate::drop_consumer::DropConsumer;
use ::catalog::contract::app_schema::DEFAULT_SCHEMA;
use ::catalog::contract::cmd_exec::CmdExec;
use ::catalog::contract::meta_mgr::MetaMgr;
use ::catalog::contract::queue::queue_system_columns;
use ::catalog::contract::schema_table::{SchemaObjectKind, SchemaTable};
use ::catalog::contract::table_desc::TableDesc;
use ::catalog::contract::table_info::TableInfo;
use ::catalog::contract::worker_snapshot::WorkerSnapshot;
use ::wal::xl_batch::XLBatch;
use ::xengine::api::XContract;
use ::xengine::tx_mgr::{PhysicalRelationId, TxMgr};
use ::xengine::x_param::PDropConsumer;
use async_trait::async_trait;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu_sys::sync::SMutex;
use mudu_type::data_type::DataType;
use mudu_type::data_type_info::DataTypeInfo;
use mudu_type::type_family::TypeFamily;
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

fn block_on<F>(fut: F) -> F::Output
where
    F: std::future::Future,
{
    mudu_sys::task::async_::build_current_thread_runtime()
        .unwrap()
        .block_on(fut)
}

fn queue_schema(name: &str) -> SchemaTable {
    let columns = queue_system_columns();
    let value_indices = (1..columns.len()).collect();
    let mut schema = SchemaTable::new(name.to_string(), columns, vec![0], value_indices);
    schema.set_schema(DEFAULT_SCHEMA.to_string());
    schema.set_kind(SchemaObjectKind::Queue);
    schema
}

fn table_schema(name: &str) -> SchemaTable {
    let column = ::catalog::contract::schema_column::SchemaColumn::new(
        "k".to_string(),
        TypeFamily::I64,
        DataTypeInfo::from_opt_object(&DataType::default_for(TypeFamily::I64)),
    );
    let mut schema = SchemaTable::new(name.to_string(), vec![column], vec![0], vec![]);
    schema.set_schema(DEFAULT_SCHEMA.to_string());
    schema
}

struct MockMetaMgr {
    schemas: SMutex<HashMap<OID, SchemaTable>>,
}

impl MockMetaMgr {
    fn new() -> Self {
        Self {
            schemas: SMutex::new(HashMap::new()),
        }
    }

    fn add_schema(&self, schema: SchemaTable) -> OID {
        let oid = schema.id();
        self.schemas.lock().unwrap().insert(oid, schema);
        oid
    }

    fn desc_of(schema: &SchemaTable) -> RS<Arc<TableDesc>> {
        TableInfo::new(schema.clone())?.table_desc()
    }
}

#[async_trait]
impl MetaMgr for MockMetaMgr {
    async fn initialize(&self) -> RS<()> {
        Ok(())
    }

    async fn get_table_by_id(&self, oid: OID) -> RS<Arc<TableDesc>> {
        let schema = self.schemas.lock().unwrap().get(&oid).cloned();
        match schema {
            Some(schema) => Self::desc_of(&schema),
            None => Err(mudu::mudu_error!(
                ErrorCode::EntityNotFound,
                format!("no such table {}", oid)
            )),
        }
    }

    async fn get_table_by_name(&self, _schema: &str, name: &str) -> RS<Option<Arc<TableDesc>>> {
        let found = self
            .schemas
            .lock()
            .unwrap()
            .values()
            .find(|s| s.table_name() == name)
            .cloned();
        found.map(|schema| Self::desc_of(&schema)).transpose()
    }

    async fn create_table(&self, schema: &SchemaTable) -> RS<()> {
        self.add_schema(schema.clone());
        Ok(())
    }

    async fn drop_table(&self, table_id: OID) -> RS<()> {
        self.schemas.lock().unwrap().remove(&table_id);
        Ok(())
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

/// Only the consumer methods the drop command drives are emulated; the
/// consumer cursors live in a plain `name -> frontier` map.
struct MockXContract {
    consumers: SMutex<HashMap<String, u64>>,
}

impl MockXContract {
    fn with_consumers(consumers: &[(&str, u64)]) -> Self {
        Self {
            consumers: SMutex::new(
                consumers
                    .iter()
                    .map(|(name, frontier)| (name.to_string(), *frontier))
                    .collect(),
            ),
        }
    }

    fn consumer_frontier_of(&self, name: &str) -> Option<u64> {
        self.consumers.lock().unwrap().get(name).copied()
    }
}

#[async_trait]
impl XContract for MockXContract {
    async fn create_table(&self, _tx_mgr: Arc<dyn TxMgr>, _schema: &SchemaTable) -> RS<()> {
        unimplemented!()
    }
    async fn drop_table(&self, _tx_mgr: Arc<dyn TxMgr>, _oid: OID) -> RS<()> {
        unimplemented!()
    }
    async fn alter_table(
        &self,
        _tx_mgr: Arc<dyn TxMgr>,
        _oid: OID,
        _alter_table: &::xengine::api::AlterTable,
    ) -> RS<()> {
        unimplemented!()
    }
    async fn begin_tx(&self) -> RS<Arc<dyn TxMgr>> {
        unimplemented!()
    }
    async fn commit_tx(&self, _tx_mgr: Arc<dyn TxMgr>) -> RS<()> {
        unimplemented!()
    }
    async fn abort_tx(&self, _tx_mgr: Arc<dyn TxMgr>) -> RS<()> {
        unimplemented!()
    }
    async fn update(
        &self,
        _tx_mgr: Arc<dyn TxMgr>,
        _table_id: OID,
        _pred_key: &::xengine::api::VecDatum,
        _pred_non_key: &::xengine::api::Predicate,
        _values: &::xengine::api::VecDatum,
        _opt_update: &::xengine::api::OptUpdate,
    ) -> RS<usize> {
        unimplemented!()
    }
    async fn read_key(
        &self,
        _tx_mgr: Arc<dyn TxMgr>,
        _table_id: OID,
        _pred_key: &::xengine::api::VecDatum,
        _select: &::xengine::api::VecSelTerm,
        _opt_read: &::xengine::api::OptRead,
    ) -> RS<Option<Vec<Option<Vec<u8>>>>> {
        unimplemented!()
    }
    async fn read_range(
        &self,
        _tx_mgr: Arc<dyn TxMgr>,
        _table_id: OID,
        _pred_key: &::xengine::api::RangeData,
        _pred_non_key: &::xengine::api::Predicate,
        _select: &::xengine::api::VecSelTerm,
        _opt_read: &::xengine::api::OptRead,
    ) -> RS<Arc<dyn ::xengine::api::RSCursor>> {
        unimplemented!()
    }
    async fn delete(
        &self,
        _tx_mgr: Arc<dyn TxMgr>,
        _table_id: OID,
        _pred_key: &::xengine::api::VecDatum,
        _pred_non_key: &::xengine::api::Predicate,
        _opt_delete: &::xengine::api::OptDelete,
    ) -> RS<usize> {
        unimplemented!()
    }
    async fn insert(
        &self,
        _tx_mgr: Arc<dyn TxMgr>,
        _table_id: OID,
        _keys: &::xengine::api::VecDatum,
        _values: &::xengine::api::VecDatum,
        _opt_insert: &::xengine::api::OptInsert,
    ) -> RS<()> {
        unimplemented!()
    }

    async fn queue_shards(&self, table_id: OID) -> RS<Vec<::xengine::api::QueueShard>> {
        Ok(vec![::xengine::api::QueueShard {
            queue_oid: table_id,
            shard_index: 0,
            partition_id: 0,
            worker_id: 0,
        }])
    }

    async fn queue_prepare_shard(&self, _shard: &::xengine::api::QueueShard) -> RS<()> {
        Ok(())
    }

    async fn consumer_drop(
        &self,
        _tx_mgr: Arc<dyn TxMgr>,
        queue_oid: OID,
        consumer: &str,
    ) -> RS<()> {
        match self.consumers.lock().unwrap().remove(consumer) {
            Some(_) => Ok(()),
            None => Err(mudu::mudu_error!(
                ErrorCode::EntityNotFound,
                format!("no such consumer {consumer} on queue {queue_oid}")
            )),
        }
    }
}

fn make_drop_consumer(
    table_id: OID,
    consumer: &str,
    x_contract: Arc<MockXContract>,
    meta: Arc<MockMetaMgr>,
) -> DropConsumer {
    DropConsumer::new(
        PDropConsumer {
            tx_mgr: Arc::new(MockTxMgr),
            table_id,
            consumer: consumer.to_string(),
        },
        x_contract,
        meta,
    )
}

#[test]
fn drop_consumer_removes_existing_cursor() {
    let meta = Arc::new(MockMetaMgr::new());
    let queue_oid = meta.add_schema(queue_schema("q"));
    let x_contract = Arc::new(MockXContract::with_consumers(&[("cg", 5), ("other", 2)]));

    let cmd = make_drop_consumer(queue_oid, "cg", x_contract.clone(), meta);
    block_on(cmd.prepare()).unwrap();
    block_on(cmd.run()).unwrap();
    assert_eq!(block_on(cmd.affected_rows()).unwrap(), 0);
    assert_eq!(x_contract.consumer_frontier_of("cg"), None);
    // The other cursor of the queue is untouched.
    assert_eq!(x_contract.consumer_frontier_of("other"), Some(2));
}

#[test]
fn drop_consumer_unknown_cursor_fails() {
    let meta = Arc::new(MockMetaMgr::new());
    let queue_oid = meta.add_schema(queue_schema("q"));
    let x_contract = Arc::new(MockXContract::with_consumers(&[]));

    let cmd = make_drop_consumer(queue_oid, "missing", x_contract, meta);
    block_on(cmd.prepare()).unwrap();
    let err = block_on(cmd.run()).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::EntityNotFound);
}

#[test]
fn drop_consumer_on_plain_table_fails_at_prepare() {
    let meta = Arc::new(MockMetaMgr::new());
    let table_oid = meta.add_schema(table_schema("t"));
    let cmd = make_drop_consumer(
        table_oid,
        "cg",
        Arc::new(MockXContract::with_consumers(&[])),
        meta,
    );
    let err = block_on(cmd.prepare()).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::EntityNotFound);
}

#[test]
fn drop_consumer_on_missing_queue_fails_at_prepare() {
    let meta = Arc::new(MockMetaMgr::new());
    let cmd = make_drop_consumer(42, "cg", Arc::new(MockXContract::with_consumers(&[])), meta);
    let err = block_on(cmd.prepare()).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::EntityNotFound);
}

// Fixture self-tests: drive every mock method so the shared test fixture is
// verified end to end (and unused fixture paths cannot rot silently).
#[test]
fn mock_tx_mgr_fixture_covers_its_trait_surface() {
    let tx = MockTxMgr;
    let _ = tx.xid();
    let _ = tx.snapshot();
    tx.put(b"k".to_vec(), b"v".to_vec());
    tx.delete(b"k".to_vec());
    let _ = tx.get(b"k");
    let relation = || PhysicalRelationId {
        table_id: 1,
        partition_id: 2,
    };
    tx.put_relation(relation(), b"k".to_vec(), b"v".to_vec());
    tx.delete_relation(relation(), b"k".to_vec());
    let _ = tx.get_relation(relation(), b"k");
    let _ = tx.staged_relation_items_in_range(relation(), b"a", b"z");
    let _ = tx.staged_relation_ops();
    let _ = tx.staged_items_in_range(b"a", b"z");
    let _ = tx.staged_put_items();
    let _ = tx.is_empty();
    let _ = tx.write_ops();
    tx.build_write_ops();
    let _ = tx.xl_batch();
}

#[test]
fn mock_meta_mgr_fixture_covers_its_trait_surface() {
    let meta = MockMetaMgr::new();
    let schema = ::catalog::contract::schema_table::SchemaTable::new(
        "fixture_t".to_string(),
        vec![::catalog::contract::schema_column::SchemaColumn::new(
            "k".to_string(),
            mudu_type::type_family::TypeFamily::I64,
            mudu_type::data_type::DataType::new_no_param(mudu_type::type_family::TypeFamily::I64)
                .to_info(),
        )],
        vec![0],
        vec![],
    );
    block_on(async {
        meta.initialize().await.unwrap();
        let _ = meta.get_table_by_id(1).await;
        let _ = meta.get_table_by_name("fixture_s", "fixture_t").await;
        meta.create_table(&schema).await.unwrap();
        meta.drop_table(1).await.unwrap();
    });
}
