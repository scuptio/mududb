#![allow(clippy::unwrap_used)]
#![allow(clippy::unimplemented)]

use crate::create_consumer::CreateConsumer;
use ::catalog::contract::app_schema::DEFAULT_SCHEMA;
use ::catalog::contract::cmd_exec::CmdExec;
use ::catalog::contract::meta_mgr::MetaMgr;
use ::catalog::contract::queue::{
    decode_queue_receive_count, decode_queue_seq, decode_queue_status, queue_lsn_datum,
    queue_receive_count_datum, queue_seq_datum, queue_status_datum, queue_system_columns,
    QUEUE_STATUS_CLAIMED, QUEUE_STATUS_READY,
};
use ::catalog::contract::schema_class::{SchemaClass, SchemaClassKind};
use ::catalog::contract::schema_column::SchemaColumn;
use ::catalog::contract::table_desc::TableDesc;
use ::catalog::contract::table_info::TableInfo;
use ::catalog::contract::worker_snapshot::WorkerSnapshot;
use ::wal::xl_batch::XLBatch;
use ::xengine::api::{
    OptDelete, OptInsert, OptRead, OptUpdate, Predicate, RSCursor, RangeData, TupleRow, VecDatum,
    VecSelTerm, XContract, XContractQueue,
};
use ::xengine::tx_mgr::{PhysicalRelationId, TxMgr};
use ::xengine::x_param::PCreateConsumer;
use async_trait::async_trait;
use mudu::common::buf::Buf;
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

fn string_datum(value: &str) -> Buf {
    mudu_type::data_type_function::send_binary(
        &mudu_type::data_value::DataValue::from_string(value.to_string()),
        &DataType::default_for(TypeFamily::String),
    )
    .unwrap()
}

/// A queue row as the full attribute vector (`__seq`, `__lsn`, `status`,
/// `receive_count`, `__state_hi`, `__state_lo`, `body`).
fn queue_row(seq: u64, lsn: u64, status: u8, receive_count: i32, body: &str) -> Vec<Option<Buf>> {
    vec![
        Some(queue_seq_datum(seq).unwrap()),
        Some(queue_lsn_datum(lsn).unwrap()),
        Some(queue_status_datum(status).unwrap()),
        Some(queue_receive_count_datum(receive_count).unwrap()),
        None,
        None,
        Some(string_datum(body)),
    ]
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

    fn add_schema(&self, schema: SchemaClass) -> OID {
        let oid = schema.id();
        self.schemas.lock().unwrap().insert(oid, schema);
        oid
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
        let schemas = self.schemas.lock().unwrap();
        let exact = schemas
            .values()
            .find(|s| s.schema() == schema && s.class_name() == name)
            .cloned();
        // Mirror the catalog search path: exact hit wins, then the default
        // schema fallback.
        if exact.is_some() || schema == DEFAULT_SCHEMA {
            return exact.map(|schema| Self::desc_of(&schema)).transpose();
        }
        schemas
            .values()
            .find(|s| s.schema() == DEFAULT_SCHEMA && s.class_name() == name)
            .cloned()
            .map(|schema| Self::desc_of(&schema))
            .transpose()
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
        self.add_schema(schema.clone());
        Ok(())
    }

    async fn get_queue(&self, schema: &str, name: &str) -> RS<Option<Arc<TableDesc>>> {
        match self.get_table_by_name(schema, name).await? {
            Some(desc) if desc.is_queue() => Ok(Some(desc)),
            _ => Ok(None),
        }
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

struct MockCursor {
    rows: SMutex<std::collections::VecDeque<TupleRow>>,
}

#[async_trait]
impl RSCursor for MockCursor {
    async fn next(&self) -> RS<Option<TupleRow>> {
        Ok(self.rows.lock().unwrap().pop_front())
    }
}

/// In-memory queue rows keyed by decoded `__seq` plus the consumer cursors
/// (`name -> frontier`); emulates the contract methods the consumer commands
/// drive.
#[derive(Default)]
struct MockStore {
    rows: BTreeMap<u64, Vec<Option<Buf>>>,
    consumers: HashMap<String, u64>,
}

struct MockXContract {
    store: SMutex<MockStore>,
}

impl MockXContract {
    fn new() -> Self {
        Self {
            store: SMutex::new(MockStore::default()),
        }
    }

    fn with_rows(self, rows: Vec<Vec<Option<Buf>>>) -> Self {
        let mut table = BTreeMap::new();
        for row in rows {
            let seq = decode_queue_seq(row[0].as_ref().unwrap()).unwrap();
            table.insert(seq, row);
        }
        self.store.lock().unwrap().rows = table;
        self
    }

    fn row(&self, seq: u64) -> Option<Vec<Option<Buf>>> {
        self.store.lock().unwrap().rows.get(&seq).cloned()
    }

    fn consumer_frontier_of(&self, name: &str) -> Option<u64> {
        self.store.lock().unwrap().consumers.get(name).copied()
    }
}

#[async_trait]
impl XContract for MockXContract {
    async fn create_table(&self, _tx_mgr: Arc<dyn TxMgr>, _schema: &SchemaClass) -> RS<()> {
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
        pred_key: &VecDatum,
        _pred_non_key: &Predicate,
        values: &VecDatum,
        _opt_update: &OptUpdate,
    ) -> RS<usize> {
        let seq = decode_queue_seq(&pred_key.data()[0].1).unwrap();
        let mut store = self.store.lock().unwrap();
        let Some(row) = store.rows.get_mut(&seq) else {
            return Ok(0);
        };
        for (attr, bin) in values.data() {
            row[*attr] = Some(bin.clone());
        }
        Ok(1)
    }
    async fn read_key(
        &self,
        _tx_mgr: Arc<dyn TxMgr>,
        _table_id: OID,
        _pred_key: &VecDatum,
        _select: &VecSelTerm,
        _opt_read: &OptRead,
    ) -> RS<Option<Vec<Option<Vec<u8>>>>> {
        unimplemented!()
    }
    async fn read_range(
        &self,
        _tx_mgr: Arc<dyn TxMgr>,
        _table_id: OID,
        pred_key: &RangeData,
        _pred_non_key: &Predicate,
        select: &VecSelTerm,
        _opt_read: &OptRead,
    ) -> RS<Arc<dyn RSCursor>> {
        // Apply the inclusive `__seq` start bound of the fan-out scan.
        let start_seq = match pred_key.start() {
            std::ops::Bound::Included(items) => Some(decode_queue_seq(&items[0].1).unwrap()),
            _ => None,
        };
        let store = self.store.lock().unwrap();
        let rows = store
            .rows
            .iter()
            .filter(|(seq, _)| start_seq.is_none_or(|start| **seq >= start))
            .map(|(_, row)| {
                TupleRow::new_nullable(select.vec().iter().map(|attr| row[*attr].clone()).collect())
            })
            .collect();
        Ok(Arc::new(MockCursor {
            rows: SMutex::new(rows),
        }))
    }
    async fn delete(
        &self,
        _tx_mgr: Arc<dyn TxMgr>,
        _table_id: OID,
        pred_key: &VecDatum,
        _pred_non_key: &Predicate,
        _opt_delete: &OptDelete,
    ) -> RS<usize> {
        let seq = decode_queue_seq(&pred_key.data()[0].1).unwrap();
        let removed = self.store.lock().unwrap().rows.remove(&seq).is_some();
        Ok(usize::from(removed))
    }
    async fn insert(
        &self,
        _tx_mgr: Arc<dyn TxMgr>,
        _table_id: OID,
        _keys: &VecDatum,
        _values: &VecDatum,
        _opt_insert: &OptInsert,
    ) -> RS<()> {
        unimplemented!()
    }

    async fn update_on_partition(
        &self,
        tx_mgr: Arc<dyn TxMgr>,
        table_id: OID,
        _partition_id: OID,
        pred_key: &VecDatum,
        values: &VecDatum,
        opt: &OptUpdate,
    ) -> RS<usize> {
        self.update(
            tx_mgr,
            table_id,
            pred_key,
            &Predicate::CNF(Vec::new()),
            values,
            opt,
        )
        .await
    }

    async fn delete_on_partition(
        &self,
        tx_mgr: Arc<dyn TxMgr>,
        table_id: OID,
        _partition_id: OID,
        pred_key: &VecDatum,
    ) -> RS<usize> {
        self.delete(
            tx_mgr,
            table_id,
            pred_key,
            &Predicate::CNF(Vec::new()),
            &OptDelete::default(),
        )
        .await
    }

    async fn read_range_on_partition(
        &self,
        tx_mgr: Arc<dyn TxMgr>,
        table_id: OID,
        _partition_id: OID,
        range: &RangeData,
        select: &VecSelTerm,
        opt: &OptRead,
    ) -> RS<Arc<dyn RSCursor>> {
        self.read_range(
            tx_mgr,
            table_id,
            range,
            &Predicate::CNF(Vec::new()),
            select,
            opt,
        )
        .await
    }
}

#[async_trait]
impl XContractQueue for MockXContract {
    async fn queue_claim_round(
        &self,
        _tx: Arc<dyn TxMgr>,
        _shard: &::xengine::api::QueueShard,
        count: u64,
    ) -> RS<Vec<::xengine::api::QueueClaimedRow>> {
        let mut store = self.store.lock().unwrap();
        let mut claimed = Vec::new();
        for (seq, row) in store.rows.iter_mut() {
            if claimed.len() as u64 >= count {
                break;
            }
            let status = decode_queue_status(row[2].as_ref().unwrap()).unwrap();
            if status != QUEUE_STATUS_READY {
                continue;
            }
            row[2] = Some(queue_status_datum(QUEUE_STATUS_CLAIMED).unwrap());
            let rc = decode_queue_receive_count(row[3].as_ref().unwrap()).unwrap() + 1;
            row[3] = Some(queue_receive_count_datum(rc).unwrap());
            claimed.push(::xengine::api::QueueClaimedRow {
                seq: *seq,
                receive_count: rc,
                fields: row.clone(),
            });
        }
        Ok(claimed)
    }

    async fn queue_consumer_scan(
        &self,
        _tx: Arc<dyn TxMgr>,
        shard: &::xengine::api::QueueShard,
        consumer: &str,
        count: u64,
    ) -> RS<Vec<Vec<Option<Buf>>>> {
        let frontier = self
            .consumer_frontier(shard, consumer)
            .await?
            .ok_or_else(|| mudu::mudu_error!(ErrorCode::ObjectNotFound, "no such consumer"))?;
        let store = self.store.lock().unwrap();
        Ok(store
            .rows
            .iter()
            .filter(|(seq, _)| **seq >= frontier)
            .take(count as usize)
            .map(|(_, row)| row.clone())
            .collect())
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

    async fn consumer_create(
        &self,
        _tx_mgr: Arc<dyn TxMgr>,
        queue_oid: OID,
        consumer: &str,
    ) -> RS<()> {
        let mut store = self.store.lock().unwrap();
        if store.consumers.contains_key(consumer) {
            return Err(mudu::mudu_error!(
                ErrorCode::ObjectAlreadyExists,
                format!("consumer {consumer} already exists on queue {queue_oid}")
            ));
        }
        store.consumers.insert(consumer.to_string(), 0);
        Ok(())
    }

    async fn consumer_drop(
        &self,
        _tx_mgr: Arc<dyn TxMgr>,
        queue_oid: OID,
        consumer: &str,
    ) -> RS<()> {
        match self.store.lock().unwrap().consumers.remove(consumer) {
            Some(_) => Ok(()),
            None => Err(mudu::mudu_error!(
                ErrorCode::ObjectNotFound,
                format!("no such consumer {consumer} on queue {queue_oid}")
            )),
        }
    }

    async fn consumers_exist(&self, _queue_oid: OID) -> RS<bool> {
        Ok(!self.store.lock().unwrap().consumers.is_empty())
    }

    async fn consumer_frontier(
        &self,
        shard: &::xengine::api::QueueShard,
        consumer: &str,
    ) -> RS<Option<u64>> {
        let _ = shard;
        Ok(self.store.lock().unwrap().consumers.get(consumer).copied())
    }
}

fn make_create_consumer(
    table_id: OID,
    consumer: &str,
    x_contract: Arc<MockXContract>,
    meta: Arc<MockMetaMgr>,
) -> CreateConsumer {
    CreateConsumer::new(
        PCreateConsumer {
            tx_mgr: Arc::new(MockTxMgr),
            table_id,
            consumer: consumer.to_string(),
        },
        x_contract,
        meta,
    )
}

#[test]
fn create_consumer_registers_cursor_and_resets_claimed_rows() {
    let meta = Arc::new(MockMetaMgr::new());
    let queue_id = meta.add_schema(queue_schema(
        "q",
        vec![payload_column("body", TypeFamily::String)],
    ));
    let x_contract = Arc::new(MockXContract::new().with_rows(vec![
        queue_row(0, 100, QUEUE_STATUS_READY, 0, "a"),
        queue_row(1, 100, QUEUE_STATUS_CLAIMED, 2, "b"),
    ]));

    let cmd = make_create_consumer(queue_id, "cg", x_contract.clone(), meta);
    block_on(cmd.prepare()).unwrap();
    block_on(cmd.run()).unwrap();
    assert_eq!(block_on(cmd.affected_rows()).unwrap(), 0);
    assert_eq!(x_contract.consumer_frontier_of("cg"), Some(0));
    // The claimed leftover is back to ready with a zeroed receive count.
    let row = x_contract.row(1).unwrap();
    assert_eq!(
        decode_queue_status(row[2].as_ref().unwrap()).unwrap(),
        QUEUE_STATUS_READY
    );
    assert_eq!(
        decode_queue_receive_count(row[3].as_ref().unwrap()).unwrap(),
        0
    );
}

#[test]
fn create_consumer_duplicate_fails() {
    let meta = Arc::new(MockMetaMgr::new());
    let queue_id = meta.add_schema(queue_schema("q", Vec::new()));
    let x_contract = Arc::new(MockXContract::new());

    let cmd = make_create_consumer(queue_id, "cg", x_contract.clone(), meta.clone());
    block_on(cmd.run()).unwrap();
    let cmd = make_create_consumer(queue_id, "cg", x_contract, meta);
    let err = block_on(cmd.run()).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::ObjectAlreadyExists);
}

#[test]
fn create_consumer_on_plain_table_fails_at_prepare() {
    let meta = Arc::new(MockMetaMgr::new());
    let table_id = meta.add_schema(table_schema("t"));
    let cmd = make_create_consumer(table_id, "cg", Arc::new(MockXContract::new()), meta);
    let err = block_on(cmd.prepare()).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::ObjectNotFound);
}

#[test]
fn create_consumer_and_receive_for_end_to_end_through_conn_core() {
    // Consumer DDL is not admin-gated, so a regular session drives the full
    // parse -> bind -> plan -> prepare -> run chain.
    let meta = Arc::new(MockMetaMgr::new());
    meta.add_schema(queue_schema(
        "q",
        vec![payload_column("body", TypeFamily::String)],
    ));
    let x_contract = Arc::new(MockXContract::new().with_rows(vec![
        queue_row(0, 100, QUEUE_STATUS_READY, 0, "m1"),
        queue_row(1, 100, QUEUE_STATUS_READY, 0, "m2"),
    ]));
    let core = MuduConnCore::new(meta.clone(), None, false).unwrap();

    let stmt = core.parse_one_text("create consumer cg on q;").unwrap();
    let affected = block_on(core.execute(
        &stmt,
        Box::new(()),
        Arc::new(MockTxMgr),
        x_contract.clone(),
        DEFAULT_SCHEMA,
    ))
    .unwrap();
    assert_eq!(affected, 0);
    assert_eq!(x_contract.consumer_frontier_of("cg"), Some(0));

    // The named cursor sees the whole queue, envelope columns included.
    let stmt = core.parse_one_text("receive 2 from q for cg;").unwrap();
    let (rows, desc) = block_on(core.query_rows(
        &stmt,
        Box::new(()),
        Arc::new(MockTxMgr),
        x_contract.clone(),
        DEFAULT_SCHEMA,
    ))
    .unwrap();
    let names: Vec<&str> = desc.fields().iter().map(|f| f.name()).collect();
    assert_eq!(names, vec!["__seq", "__lsn", "body"]);
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].values()[0].to_i64(), 0);
    assert_eq!(rows[1].values()[0].to_i64(), 1);

    // Fan-out mode disables the default competing receive.
    let stmt = core.parse_one_text("receive 1 from q;").unwrap();
    let err = block_on(core.query_rows(
        &stmt,
        Box::new(()),
        Arc::new(MockTxMgr),
        x_contract.clone(),
        DEFAULT_SCHEMA,
    ))
    .unwrap_err();
    assert_eq!(err.ec(), ErrorCode::InvalidArgument);

    let stmt = core.parse_one_text("drop consumer cg on q;").unwrap();
    block_on(core.execute(
        &stmt,
        Box::new(()),
        Arc::new(MockTxMgr),
        x_contract.clone(),
        DEFAULT_SCHEMA,
    ))
    .unwrap();
    assert_eq!(x_contract.consumer_frontier_of("cg"), None);

    // With the last cursor gone the queue is back to competing mode.
    let stmt = core.parse_one_text("receive 1 from q;").unwrap();
    let (rows, _) = block_on(core.query_rows(
        &stmt,
        Box::new(()),
        Arc::new(MockTxMgr),
        x_contract.clone(),
        DEFAULT_SCHEMA,
    ))
    .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].values()[0].to_i64(), 0);
    // The competing receive claimed the row again.
    let row = x_contract.row(0).unwrap();
    assert_eq!(
        decode_queue_status(row[2].as_ref().unwrap()).unwrap(),
        QUEUE_STATUS_CLAIMED
    );
}
