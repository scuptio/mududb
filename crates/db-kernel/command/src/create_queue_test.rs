#![allow(clippy::unwrap_used)]
#![allow(clippy::unimplemented)]
use crate::create_queue::CreateQueue;
use ::catalog::contract::app_schema::DEFAULT_SCHEMA;
use ::catalog::contract::cmd_exec::CmdExec;
use ::catalog::contract::meta_mgr::MetaMgr;
use ::catalog::contract::queue::{
    queue_system_columns, QUEUE_LSN_COLUMN, QUEUE_RECEIVE_COUNT_COLUMN, QUEUE_SEQ_COLUMN,
    QUEUE_STATUS_COLUMN,
};
use ::catalog::contract::schema_class::{SchemaClass, SchemaClassKind};
use ::catalog::contract::schema_column::SchemaColumn;
use ::catalog::contract::table_desc::TableDesc;
use ::catalog::contract::table_info::TableInfo;
use ::catalog::contract::worker_snapshot::WorkerSnapshot;
use ::wal::xl_batch::XLBatch;
use ::xengine::api::{XContract, XContractQueue};
use ::xengine::tx_mgr::{PhysicalRelationId, TxMgr};
use ::xengine::x_param::PCreateQueue;
use async_trait::async_trait;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu_kernel::mudu_conn::mudu_conn_core::MuduConnCore;
use mudu_sys::sync::SMutex;
use mudu_type::data_type::DataType;
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

fn payload_column(name: &str, family: TypeFamily) -> SchemaColumn {
    SchemaColumn::new(
        name.to_string(),
        family,
        mudu_type::data_type_codec::from_opt_object(&DataType::default_for(family)),
    )
}

/// Builds a queue schema the way the binder does: the four system columns
/// first, then the payload columns; key is `__seq` alone.
fn queue_schema(name: &str, payload: Vec<SchemaColumn>) -> SchemaClass {
    let mut columns = queue_system_columns();
    columns.extend(payload);
    let value_indices = (1..columns.len()).collect();
    let mut schema = SchemaClass::new(name.to_string(), columns, vec![0], value_indices);
    schema.set_schema(DEFAULT_SCHEMA.to_string());
    schema.set_kind(SchemaClassKind::Queue);
    schema
}

fn table_schema(name: &str) -> SchemaClass {
    let mut schema = SchemaClass::new(
        name.to_string(),
        vec![payload_column("k", TypeFamily::I64)],
        vec![0],
        vec![],
    );
    schema.set_schema(DEFAULT_SCHEMA.to_string());
    schema
}

struct MockMetaMgr {
    schemas: SMutex<HashMap<OID, SchemaClass>>,
}

impl MockMetaMgr {
    fn new() -> Self {
        Self {
            schemas: SMutex::new(HashMap::new()),
        }
    }

    fn add_schema(&self, schema: SchemaClass) {
        self.schemas.lock().unwrap().insert(schema.id(), schema);
    }

    fn find_by_name(&self, schema: &str, name: &str) -> Option<SchemaClass> {
        let schemas = self.schemas.lock().unwrap();
        let exact = schemas
            .values()
            .find(|s| s.schema() == schema && s.class_name() == name)
            .cloned();
        // Mirror the catalog search path: exact hit wins, then the default
        // schema fallback.
        if exact.is_some() || schema == DEFAULT_SCHEMA {
            return exact;
        }
        schemas
            .values()
            .find(|s| s.schema() == DEFAULT_SCHEMA && s.class_name() == name)
            .cloned()
    }

    fn desc_of(schema: &SchemaClass) -> RS<Arc<TableDesc>> {
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
                ErrorCode::ObjectNotFound,
                format!("no such table {}", oid)
            )),
        }
    }

    async fn get_table_by_name(&self, schema: &str, name: &str) -> RS<Option<Arc<TableDesc>>> {
        self.find_by_name(schema, name)
            .map(|schema| Self::desc_of(&schema))
            .transpose()
    }

    async fn get_table_exact(&self, schema: &str, name: &str) -> RS<Option<Arc<TableDesc>>> {
        let found = self
            .schemas
            .lock()
            .unwrap()
            .values()
            .find(|s| s.schema() == schema && s.class_name() == name)
            .cloned();
        found.map(|schema| Self::desc_of(&schema)).transpose()
    }

    async fn create_table(&self, schema: &SchemaClass) -> RS<()> {
        self.add_schema(schema.clone());
        Ok(())
    }

    async fn drop_table(&self, table_id: OID) -> RS<()> {
        self.schemas.lock().unwrap().remove(&table_id);
        Ok(())
    }

    async fn create_queue(&self, schema: &SchemaClass) -> RS<()> {
        if schema.kind() != SchemaClassKind::Queue {
            return Err(mudu::mudu_error!(
                ErrorCode::InvalidArgument,
                "schema does not carry the queue kind"
            ));
        }
        if self
            .find_by_name(schema.schema(), schema.class_name())
            .is_some()
        {
            return Err(mudu::mudu_error!(
                ErrorCode::ObjectAlreadyExists,
                format!("queue {} already exists", schema.class_name())
            ));
        }
        self.add_schema(schema.clone());
        Ok(())
    }

    async fn drop_queue(&self, queue_id: OID) -> RS<()> {
        let schema = self.schemas.lock().unwrap().get(&queue_id).cloned();
        match schema {
            Some(schema) if schema.is_queue() => {
                self.schemas.lock().unwrap().remove(&queue_id);
                Ok(())
            }
            _ => Err(mudu::mudu_error!(
                ErrorCode::ObjectNotFound,
                format!("no such queue {}", queue_id)
            )),
        }
    }

    async fn get_queue(&self, schema: &str, name: &str) -> RS<Option<Arc<TableDesc>>> {
        match self.find_by_name(schema, name) {
            Some(schema) if schema.is_queue() => Self::desc_of(&schema).map(Some),
            _ => Ok(None),
        }
    }

    async fn list_queues(&self) -> RS<Vec<SchemaClass>> {
        Ok(self
            .schemas
            .lock()
            .unwrap()
            .values()
            .filter(|schema| schema.is_queue())
            .cloned()
            .collect())
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

/// Mirrors the real `WorkerXContract` chain: `create_queue` writes the
/// catalog through the meta mgr.
struct MockXContract {
    meta: Arc<MockMetaMgr>,
}

#[async_trait]
impl XContract for MockXContract {
    async fn create_table(&self, _tx_mgr: Arc<dyn TxMgr>, _schema: &SchemaClass) -> RS<()> {
        unimplemented!()
    }
    async fn drop_table(&self, _tx_mgr: Arc<dyn TxMgr>, _oid: OID) -> RS<()> {
        unimplemented!()
    }
    async fn create_queue(&self, _tx_mgr: Arc<dyn TxMgr>, schema: &SchemaClass) -> RS<()> {
        self.meta.create_queue(schema).await
    }
    async fn drop_queue(&self, _tx_mgr: Arc<dyn TxMgr>, oid: OID) -> RS<()> {
        self.meta.drop_queue(oid).await
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
        struct EmptyCursor;
        #[async_trait]
        impl ::xengine::api::RSCursor for EmptyCursor {
            async fn next(&self) -> RS<Option<::xengine::api::TupleRow>> {
                Ok(None)
            }
        }
        Ok(Arc::new(EmptyCursor))
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
}

#[async_trait]
impl XContractQueue for MockXContract {
    async fn queue_shards(&self, table_id: OID) -> RS<Vec<::xengine::api::QueueShard>> {
        Ok(vec![::xengine::api::QueueShard {
            queue_oid: table_id,
            shard_index: 0,
            partition_id: 0,
            worker_id: 0,
        }])
    }

    async fn queue_claim_round(
        &self,
        _tx: Arc<dyn TxMgr>,
        _shard: &::xengine::api::QueueShard,
        _count: u64,
    ) -> RS<Vec<::xengine::api::QueueClaimedRow>> {
        Ok(Vec::new())
    }

    async fn queue_prepare_shard(&self, _shard: &::xengine::api::QueueShard) -> RS<()> {
        Ok(())
    }

    async fn consumers_exist(&self, _queue_oid: OID) -> RS<bool> {
        Ok(false)
    }
}

fn make_create_queue(schema: SchemaClass, meta: Arc<MockMetaMgr>) -> CreateQueue {
    CreateQueue::new(
        PCreateQueue {
            tx_mgr: Arc::new(MockTxMgr),
            schema,
            partition_binding: None,
        },
        Arc::new(MockXContract { meta: meta.clone() }),
        meta,
    )
}

#[test]
fn create_queue_creates_queue_with_system_column_layout() {
    let meta = Arc::new(MockMetaMgr::new());
    let schema = queue_schema("q", vec![payload_column("body", TypeFamily::String)]);
    let queue_oid = schema.id();

    let cmd = make_create_queue(schema, meta.clone());
    block_on(cmd.prepare()).unwrap();
    block_on(cmd.run()).unwrap();
    assert_eq!(block_on(cmd.affected_rows()).unwrap(), 0);

    let desc = block_on(meta.get_queue(DEFAULT_SCHEMA, "q"))
        .unwrap()
        .expect("queue must exist after create");
    assert_eq!(desc.id(), queue_oid);
    assert!(desc.is_queue());
    // Key layout: the `__seq` column alone.
    assert_eq!(desc.key_indices(), &vec![0]);
    assert_eq!(
        desc.value_indices(),
        &(1..7).collect::<Vec<_>>(),
        "value layout is the five system headers plus the payload column"
    );
    let fields = desc.fields();
    let expect = [
        (QUEUE_SEQ_COLUMN, TypeFamily::I64, false),
        (QUEUE_LSN_COLUMN, TypeFamily::I64, false),
        (QUEUE_STATUS_COLUMN, TypeFamily::I32, false),
        (QUEUE_RECEIVE_COUNT_COLUMN, TypeFamily::I32, false),
        ("__state_hi", TypeFamily::U128, true),
        ("__state_lo", TypeFamily::U128, true),
        ("body", TypeFamily::String, true),
    ];
    assert_eq!(fields.len(), expect.len());
    for (field, (name, family, nullable)) in fields.iter().zip(expect.iter()) {
        assert_eq!(field.name(), name);
        assert_eq!(field.type_desc().type_family(), *family);
        assert_eq!(field.nullable(), *nullable);
    }
    assert!(fields[0].is_primary(), "__seq is the queue primary key");
    // CREATE QUEUE derives the dead-letter queue `q_dlq` with the same
    // payload layout.
    let dlq = block_on(meta.get_queue(DEFAULT_SCHEMA, "q_dlq"))
        .unwrap()
        .expect("the dead-letter queue is derived");
    assert!(dlq.is_queue());
    assert_eq!(dlq.fields().len(), 7);
    assert_eq!(dlq.fields()[6].name(), "body");
    assert_eq!(block_on(meta.list_queues()).unwrap().len(), 2);
}

#[test]
fn create_queue_prepare_fails_on_name_collision_with_table() {
    let meta = Arc::new(MockMetaMgr::new());
    meta.add_schema(table_schema("dup"));

    let cmd = make_create_queue(queue_schema("dup", Vec::new()), meta);
    let err = block_on(cmd.prepare()).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::ObjectAlreadyExists);
}

#[test]
fn create_queue_prepare_fails_on_name_collision_with_queue() {
    let meta = Arc::new(MockMetaMgr::new());
    meta.add_schema(queue_schema("dup", Vec::new()));

    let cmd = make_create_queue(queue_schema("dup", Vec::new()), meta);
    let err = block_on(cmd.prepare()).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::ObjectAlreadyExists);
}

#[test]
fn create_queue_run_fails_when_queue_created_concurrently() {
    let meta = Arc::new(MockMetaMgr::new());
    let cmd = make_create_queue(queue_schema("q", Vec::new()), meta.clone());
    block_on(cmd.prepare()).unwrap();
    // A same-named queue lands between prepare and run; the catalog write in
    // run rejects it.
    meta.add_schema(queue_schema("q", Vec::new()));
    let err = block_on(cmd.run()).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::ObjectAlreadyExists);
}

#[test]
fn create_queue_rejects_schema_without_queue_kind() {
    let meta = Arc::new(MockMetaMgr::new());
    let cmd = make_create_queue(table_schema("plain"), meta);
    block_on(cmd.prepare()).unwrap();
    let err = block_on(cmd.run()).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::InvalidArgument);
}

#[test]
fn create_and_drop_queue_end_to_end_through_conn_core() {
    // Queue DDL is not admin-gated (only fs-type DDL is), so a regular
    // session drives the full parse -> bind -> plan -> prepare -> run chain.
    let meta = Arc::new(MockMetaMgr::new());
    let core = MuduConnCore::new(meta.clone(), None, false).unwrap();

    let stmt = core
        .parse_one_text("create queue jobs (body varchar(100));")
        .unwrap();
    let affected = block_on(core.execute(
        &stmt,
        Box::new(()),
        Arc::new(MockTxMgr),
        Arc::new(MockXContract { meta: meta.clone() }),
        DEFAULT_SCHEMA,
    ))
    .unwrap();
    assert_eq!(affected, 0);
    let desc = block_on(meta.get_queue(DEFAULT_SCHEMA, "jobs"))
        .unwrap()
        .expect("queue exists after CREATE QUEUE");
    assert!(desc.is_queue());
    assert_eq!(desc.fields().len(), 7);

    // RECEIVE is a row-returning statement: it goes through the query path
    // and yields envelope columns (`__seq`, `__lsn`, payload).
    let stmt = core.parse_one_text("receive from jobs;").unwrap();
    let rs = block_on(core.query(
        &stmt,
        Box::new(()),
        Arc::new(MockTxMgr),
        Arc::new(MockXContract { meta: meta.clone() }),
        DEFAULT_SCHEMA,
    ))
    .unwrap();
    let desc = rs.desc();
    let names: Vec<&str> = desc.fields().iter().map(|f| f.name()).collect();
    assert_eq!(names, vec!["__seq", "__lsn", "body"]);

    // RECEIVE through the command channel is rejected like a SELECT is.
    let stmt = core.parse_one_text("receive 1 from jobs;").unwrap();
    let err = block_on(core.execute(
        &stmt,
        Box::new(()),
        Arc::new(MockTxMgr),
        Arc::new(MockXContract { meta: meta.clone() }),
        DEFAULT_SCHEMA,
    ))
    .unwrap_err();
    assert_eq!(err.ec(), ErrorCode::InvalidType);

    let stmt = core.parse_one_text("drop queue jobs;").unwrap();
    block_on(core.execute(
        &stmt,
        Box::new(()),
        Arc::new(MockTxMgr),
        Arc::new(MockXContract { meta: meta.clone() }),
        DEFAULT_SCHEMA,
    ))
    .unwrap();
    assert!(
        block_on(meta.get_queue(DEFAULT_SCHEMA, "jobs"))
            .unwrap()
            .is_none(),
        "queue is gone after DROP QUEUE"
    );
    assert!(
        block_on(meta.get_queue(DEFAULT_SCHEMA, "jobs_dlq"))
            .unwrap()
            .is_none(),
        "the derived dead-letter queue goes with its queue"
    );
}
