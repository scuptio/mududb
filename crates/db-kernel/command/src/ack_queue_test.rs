#![allow(clippy::unwrap_used)]

use crate::ack_queue::AckQueue;
use ::catalog::contract::app_schema::DEFAULT_SCHEMA;
use ::catalog::contract::cmd_exec::CmdExec;
use ::catalog::contract::meta_mgr::MetaMgr;
use ::catalog::contract::queue::{
    decode_queue_receive_count, decode_queue_seq, decode_queue_status, queue_lsn_datum,
    queue_receive_count_datum, queue_seq_datum, queue_status_datum, QUEUE_LSN_ATTR,
    QUEUE_PAYLOAD_ATTR_START, QUEUE_POISON_RECEIVE_COUNT, QUEUE_RECEIVE_COUNT_ATTR, QUEUE_SEQ_ATTR,
    QUEUE_STATUS_ATTR, QUEUE_STATUS_CLAIMED, QUEUE_STATUS_READY,
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
use ::xengine::x_param::PAckQueue;
use async_trait::async_trait;
use mudu::common::buf::Buf;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
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

fn queue_schema(name: &str) -> SchemaClass {
    let mut columns = ::catalog::contract::queue::queue_system_columns();
    columns.push(SchemaColumn::new(
        "body".to_string(),
        TypeFamily::String,
        mudu_type::data_type_codec::from_opt_object(&DataType::default_for(TypeFamily::String)),
    ));
    let value_indices = (1..columns.len()).collect();
    let mut schema = SchemaClass::new(name.to_string(), columns, vec![0], value_indices);
    schema.set_schema(DEFAULT_SCHEMA.to_string());
    schema.set_kind(SchemaClassKind::Queue);
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

struct MockTxMgr;

impl TxMgr for MockTxMgr {
    fn xid(&self) -> u64 {
        9
    }
    fn snapshot(&self) -> WorkerSnapshot {
        WorkerSnapshot::new(9, Vec::new())
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

/// In-memory queue tables keyed by decoded `__seq`, plus the queue runtime
/// counters the ACK executor drives. `consumers` holds the named consumer
/// cursors per queue (`name -> frontier`, fan-out mode).
#[derive(Default)]
struct MockStore {
    tables: HashMap<OID, BTreeMap<u64, Vec<Option<Buf>>>>,
    next_seq: HashMap<OID, u64>,
    frontier: HashMap<OID, u64>,
    consumers: HashMap<OID, HashMap<String, u64>>,
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

struct MockXContract {
    store: SMutex<MockStore>,
}

impl MockXContract {
    fn new(queue_id: OID, rows: Vec<Vec<Option<Buf>>>) -> Self {
        let mut table = BTreeMap::new();
        for row in rows {
            let seq = decode_queue_seq(row[0].as_ref().unwrap()).unwrap();
            table.insert(seq, row);
        }
        let mut tables = HashMap::new();
        tables.insert(queue_id, table);
        Self {
            store: SMutex::new(MockStore {
                tables,
                next_seq: HashMap::new(),
                frontier: HashMap::new(),
                consumers: HashMap::new(),
            }),
        }
    }

    /// Registers a consumer cursor with the given frontier on the queue
    /// (fan-out mode).
    fn with_consumer(self, queue_id: OID, name: &str, frontier: u64) -> Self {
        self.store
            .lock()
            .unwrap()
            .consumers
            .entry(queue_id)
            .or_default()
            .insert(name.to_string(), frontier);
        self
    }

    fn consumer_frontier_of(&self, table_id: OID, name: &str) -> Option<u64> {
        self.store
            .lock()
            .unwrap()
            .consumers
            .get(&table_id)
            .and_then(|cursors| cursors.get(name).copied())
    }

    fn rows_of(&self, table_id: OID) -> Vec<u64> {
        self.store
            .lock()
            .unwrap()
            .tables
            .get(&table_id)
            .map(|rows| rows.keys().copied().collect())
            .unwrap_or_default()
    }

    fn row_of(&self, table_id: OID, seq: u64) -> Option<Vec<Option<Buf>>> {
        self.store
            .lock()
            .unwrap()
            .tables
            .get(&table_id)
            .and_then(|rows| rows.get(&seq).cloned())
    }

    /// The mock hosts one shard (index 0, partition 0) per queue.
    fn mock_shard(table_id: OID) -> ::xengine::api::QueueShard {
        ::xengine::api::QueueShard {
            queue_oid: table_id,
            shard_index: 0,
            partition_id: 0,
            worker_id: 0,
        }
    }

    fn frontier_of(&self, table_id: OID) -> Option<u64> {
        self.store.lock().unwrap().frontier.get(&table_id).copied()
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
        _pred_key: &VecDatum,
        _pred_non_key: &Predicate,
        _values: &VecDatum,
        _opt_update: &OptUpdate,
    ) -> RS<usize> {
        unimplemented!()
    }
    async fn read_key(
        &self,
        _tx_mgr: Arc<dyn TxMgr>,
        _table_id: OID,
        _pred_key: &VecDatum,
        _select: &VecSelTerm,
        _opt_read: &OptRead,
    ) -> RS<Option<Vec<Option<Buf>>>> {
        unimplemented!()
    }
    async fn read_range(
        &self,
        _tx_mgr: Arc<dyn TxMgr>,
        table_id: OID,
        pred_key: &RangeData,
        _pred_non_key: &Predicate,
        select: &VecSelTerm,
        _opt_read: &OptRead,
    ) -> RS<Arc<dyn RSCursor>> {
        // Apply the inclusive `__seq` end bound of the ACK scan.
        let end_seq = match pred_key.end() {
            std::ops::Bound::Included(items) => Some(decode_queue_seq(&items[0].1).unwrap()),
            _ => None,
        };
        let store = self.store.lock().unwrap();
        let rows = store
            .tables
            .get(&table_id)
            .map(|table| {
                table
                    .iter()
                    .filter(|(seq, _)| end_seq.is_none_or(|end| **seq <= end))
                    .map(|(_, row)| {
                        TupleRow::new_nullable(
                            select.vec().iter().map(|attr| row[*attr].clone()).collect(),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        Ok(Arc::new(MockCursor {
            rows: SMutex::new(rows),
        }))
    }
    async fn delete(
        &self,
        _tx_mgr: Arc<dyn TxMgr>,
        table_id: OID,
        pred_key: &VecDatum,
        _pred_non_key: &Predicate,
        _opt_delete: &OptDelete,
    ) -> RS<usize> {
        let seq = decode_queue_seq(&pred_key.data()[0].1).unwrap();
        let mut store = self.store.lock().unwrap();
        let removed = store
            .tables
            .get_mut(&table_id)
            .map(|rows| rows.remove(&seq).is_some())
            .unwrap_or(false);
        Ok(usize::from(removed))
    }
    async fn insert(
        &self,
        _tx_mgr: Arc<dyn TxMgr>,
        table_id: OID,
        keys: &VecDatum,
        values: &VecDatum,
        _opt_insert: &OptInsert,
    ) -> RS<()> {
        let seq = decode_queue_seq(&keys.data()[0].1).unwrap();
        let width = 1 + keys
            .data()
            .iter()
            .chain(values.data().iter())
            .map(|(attr, _)| *attr)
            .max()
            .unwrap_or(0);
        let mut row = vec![None; width];
        for (attr, bin) in keys.data().iter().chain(values.data().iter()) {
            row[*attr] = Some(bin.clone());
        }
        let mut store = self.store.lock().unwrap();
        store.tables.entry(table_id).or_default().insert(seq, row);
        Ok(())
    }

    async fn read_range_on_partition(
        &self,
        _tx_mgr: Arc<dyn TxMgr>,
        table_id: OID,
        _partition_id: OID,
        pred_key: &RangeData,
        select: &VecSelTerm,
        _opt: &OptRead,
    ) -> RS<Arc<dyn RSCursor>> {
        // Apply the inclusive `__seq` end bound of the ACK scan.
        let end_seq = match pred_key.end() {
            std::ops::Bound::Included(items) => Some(decode_queue_seq(&items[0].1).unwrap()),
            _ => None,
        };
        let store = self.store.lock().unwrap();
        let rows = store
            .tables
            .get(&table_id)
            .map(|table| {
                table
                    .iter()
                    .filter(|(seq, _)| end_seq.is_none_or(|end| **seq <= end))
                    .map(|(_, row)| {
                        TupleRow::new_nullable(
                            select.vec().iter().map(|attr| row[*attr].clone()).collect(),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        Ok(Arc::new(MockCursor {
            rows: SMutex::new(rows),
        }))
    }

    async fn delete_on_partition(
        &self,
        _tx_mgr: Arc<dyn TxMgr>,
        table_id: OID,
        _partition_id: OID,
        pred_key: &VecDatum,
    ) -> RS<usize> {
        let seq = decode_queue_seq(&pred_key.data()[0].1).unwrap();
        let mut store = self.store.lock().unwrap();
        let removed = store
            .tables
            .get_mut(&table_id)
            .map(|rows| rows.remove(&seq).is_some())
            .unwrap_or(false);
        Ok(usize::from(removed))
    }
}

#[async_trait]
impl XContractQueue for MockXContract {
    async fn queue_shards(&self, table_id: OID) -> RS<Vec<::xengine::api::QueueShard>> {
        Ok(vec![Self::mock_shard(table_id)])
    }

    async fn queue_shard_of_seq(&self, table_id: OID, _seq: u64) -> RS<::xengine::api::QueueShard> {
        Ok(Self::mock_shard(table_id))
    }

    async fn queue_prepare_shard(&self, _shard: &::xengine::api::QueueShard) -> RS<()> {
        Ok(())
    }

    async fn queue_lock_shard(
        &self,
        _tx: Arc<dyn TxMgr>,
        _shard: &::xengine::api::QueueShard,
    ) -> RS<()> {
        Ok(())
    }

    async fn queue_ack_in_tx(
        &self,
        _tx: Arc<dyn TxMgr>,
        shard: &::xengine::api::QueueShard,
        local_seq: u64,
        _deleted: u64,
        _deleted_bytes: u64,
    ) -> RS<()> {
        let mut store = self.store.lock().unwrap();
        let frontier = store.frontier.entry(shard.queue_oid).or_insert(0);
        *frontier = (*frontier).max(local_seq + 1);
        Ok(())
    }

    async fn consumers_exist_in_tx(&self, _tx: Arc<dyn TxMgr>, queue_oid: OID) -> RS<bool> {
        Ok(self
            .store
            .lock()
            .unwrap()
            .consumers
            .get(&queue_oid)
            .is_some_and(|cursors| !cursors.is_empty()))
    }

    async fn consumer_frontier_in_tx(
        &self,
        _tx: Arc<dyn TxMgr>,
        shard: &::xengine::api::QueueShard,
        consumer: &str,
    ) -> RS<Option<u64>> {
        Ok(self
            .store
            .lock()
            .unwrap()
            .consumers
            .get(&shard.queue_oid)
            .and_then(|cursors| cursors.get(consumer).copied()))
    }

    /// Mirrors the real `consumer_ack_and_gc`: advance the cursor frontier to
    /// `max(frontier, seq + 1)`, then delete the queue rows at or below
    /// `min(frontier) - 1` (the prefix every consumer acknowledged).
    async fn consumer_ack_and_gc(
        &self,
        _tx_mgr: Arc<dyn TxMgr>,
        shard: &::xengine::api::QueueShard,
        consumer: &str,
        seq: u64,
    ) -> RS<u64> {
        let queue_oid = shard.queue_oid;
        let mut store = self.store.lock().unwrap();
        let Some(cursors) = store.consumers.get_mut(&queue_oid) else {
            return Err(mudu::mudu_error!(
                ErrorCode::ObjectNotFound,
                format!("no such consumer {consumer} on queue {queue_oid}")
            ));
        };
        let Some(cursor) = cursors.get_mut(consumer) else {
            return Err(mudu::mudu_error!(
                ErrorCode::ObjectNotFound,
                format!("no such consumer {consumer} on queue {queue_oid}")
            ));
        };
        *cursor = (*cursor).max(seq.saturating_add(1));
        let min_frontier = cursors.values().copied().min().unwrap_or(0);
        if min_frontier == 0 {
            return Ok(0);
        }
        let gc_seq = min_frontier - 1;
        let rows = store.tables.entry(queue_oid).or_default();
        let before = rows.len();
        rows.retain(|seq, _| *seq > gc_seq);
        Ok((before - rows.len()) as u64)
    }

    /// Mirrors the real transfer: a fresh enqueue into the DLQ table with the
    /// payload copied, then the source row is deleted.
    async fn queue_dlq_transfer(
        &self,
        _tx: Arc<dyn TxMgr>,
        source: &::xengine::api::QueueShard,
        dlq: &::xengine::api::QueueShard,
        fields: &[Option<Buf>],
    ) -> RS<()> {
        let mut store = self.store.lock().unwrap();
        let dlq_seq = {
            let next = store.next_seq.entry(dlq.queue_oid).or_insert(0);
            let seq = *next;
            *next += 1;
            seq
        };
        let mut row = vec![None; fields.len()];
        row[QUEUE_SEQ_ATTR] = Some(queue_seq_datum(dlq_seq).unwrap());
        row[QUEUE_LSN_ATTR] = fields[QUEUE_LSN_ATTR].clone();
        row[QUEUE_STATUS_ATTR] = Some(queue_status_datum(QUEUE_STATUS_READY).unwrap());
        row[QUEUE_RECEIVE_COUNT_ATTR] = Some(queue_receive_count_datum(0).unwrap());
        row[QUEUE_PAYLOAD_ATTR_START..].clone_from_slice(&fields[QUEUE_PAYLOAD_ATTR_START..]);
        store
            .tables
            .entry(dlq.queue_oid)
            .or_default()
            .insert(dlq_seq, row);
        let seq = decode_queue_seq(fields[QUEUE_SEQ_ATTR].as_ref().unwrap()).unwrap();
        store
            .tables
            .get_mut(&source.queue_oid)
            .and_then(|rows| rows.remove(&seq));
        Ok(())
    }
}

struct MockMetaMgr {
    schemas: SMutex<HashMap<OID, SchemaClass>>,
}

impl MockMetaMgr {
    fn new(queue: SchemaClass, dlq: Option<SchemaClass>) -> Self {
        let mut schemas = HashMap::new();
        schemas.insert(queue.id(), queue);
        if let Some(dlq) = dlq {
            schemas.insert(dlq.id(), dlq);
        }
        Self {
            schemas: SMutex::new(schemas),
        }
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
    async fn get_table_by_name(&self, _schema: &str, name: &str) -> RS<Option<Arc<TableDesc>>> {
        let found = self
            .schemas
            .lock()
            .unwrap()
            .values()
            .find(|schema| schema.class_name() == name)
            .cloned();
        found.map(|schema| Self::desc_of(&schema)).transpose()
    }
    async fn create_table(&self, _schema: &SchemaClass) -> RS<()> {
        Ok(())
    }
    async fn drop_table(&self, _table_id: OID) -> RS<()> {
        Ok(())
    }
    async fn get_queue(&self, schema: &str, name: &str) -> RS<Option<Arc<TableDesc>>> {
        match self.get_table_by_name(schema, name).await? {
            Some(desc) if desc.is_queue() => Ok(Some(desc)),
            _ => Ok(None),
        }
    }
}

fn make_ack(
    x_contract: Arc<MockXContract>,
    meta: Arc<MockMetaMgr>,
    table_id: OID,
    seq: u64,
) -> AckQueue {
    AckQueue::new(
        PAckQueue {
            tx_mgr: Arc::new(MockTxMgr),
            table_id,
            seq,
            consumer: None,
        },
        x_contract,
        meta,
    )
}

#[test]
fn ack_deletes_claimed_prefix_and_advances_frontier() {
    let queue = queue_schema("q");
    let queue_id = queue.id();
    let x_contract = Arc::new(MockXContract::new(
        queue_id,
        vec![
            queue_row(0, 100, QUEUE_STATUS_CLAIMED, 1, "a"),
            queue_row(1, 100, QUEUE_STATUS_CLAIMED, 1, "b"),
            queue_row(2, 100, QUEUE_STATUS_READY, 0, "c"),
            queue_row(3, 100, QUEUE_STATUS_CLAIMED, 1, "d"),
        ],
    ));
    let meta = Arc::new(MockMetaMgr::new(queue, None));
    let cmd = make_ack(x_contract.clone(), meta, queue_id, 1);
    block_on(cmd.prepare()).unwrap();
    block_on(cmd.run()).unwrap();
    assert_eq!(block_on(cmd.affected_rows()).unwrap(), 2);
    assert_eq!(
        x_contract.rows_of(queue_id),
        vec![2, 3],
        "claimed rows at or below the ack seq are deleted; the ready row and later rows stay"
    );
    assert_eq!(x_contract.frontier_of(queue_id), Some(2));
}

#[test]
fn ack_keeps_ready_rows_below_the_seq() {
    let queue = queue_schema("q");
    let queue_id = queue.id();
    let x_contract = Arc::new(MockXContract::new(
        queue_id,
        vec![
            queue_row(0, 100, QUEUE_STATUS_READY, 0, "a"),
            queue_row(1, 100, QUEUE_STATUS_CLAIMED, 1, "b"),
        ],
    ));
    let meta = Arc::new(MockMetaMgr::new(queue, None));
    let cmd = make_ack(x_contract.clone(), meta, queue_id, 1);
    block_on(cmd.run()).unwrap();
    assert_eq!(block_on(cmd.affected_rows()).unwrap(), 1);
    assert_eq!(x_contract.rows_of(queue_id), vec![0]);
}

#[test]
fn ack_moves_poison_message_to_dlq_instead_of_deleting() {
    let queue = queue_schema("q");
    let queue_id = queue.id();
    let dlq = queue_schema("q_dlq");
    let dlq_id = dlq.id();
    let x_contract = Arc::new(MockXContract::new(
        queue_id,
        vec![queue_row(
            0,
            100,
            QUEUE_STATUS_CLAIMED,
            QUEUE_POISON_RECEIVE_COUNT + 1,
            "poison",
        )],
    ));
    let meta = Arc::new(MockMetaMgr::new(queue, Some(dlq)));
    let cmd = make_ack(x_contract.clone(), meta, queue_id, 0);
    block_on(cmd.run()).unwrap();
    assert_eq!(block_on(cmd.affected_rows()).unwrap(), 1);
    assert!(x_contract.rows_of(queue_id).is_empty());

    let dlq_row = x_contract
        .row_of(dlq_id, 0)
        .expect("the poison message landed in the DLQ");
    // DLQ rows are fresh enqueues: ready status, zero receive count, the
    // acking transaction's xid as __lsn, payload copied verbatim.
    assert_eq!(
        decode_queue_status(dlq_row[2].as_ref().unwrap()).unwrap(),
        QUEUE_STATUS_READY
    );
    assert_eq!(
        decode_queue_receive_count(dlq_row[3].as_ref().unwrap()).unwrap(),
        0
    );
    assert_eq!(dlq_row[6].as_ref().unwrap(), &string_datum("poison"));
}

#[test]
fn ack_deletes_poison_message_when_dlq_is_missing() {
    let queue = queue_schema("q");
    let queue_id = queue.id();
    let x_contract = Arc::new(MockXContract::new(
        queue_id,
        vec![queue_row(
            0,
            100,
            QUEUE_STATUS_CLAIMED,
            QUEUE_POISON_RECEIVE_COUNT + 1,
            "poison",
        )],
    ));
    let meta = Arc::new(MockMetaMgr::new(queue, None));
    let cmd = make_ack(x_contract.clone(), meta, queue_id, 0);
    block_on(cmd.run()).unwrap();
    assert_eq!(block_on(cmd.affected_rows()).unwrap(), 1);
    assert!(x_contract.rows_of(queue_id).is_empty());
}

#[test]
fn ack_on_non_queue_fails_at_prepare() {
    let mut plain = SchemaClass::new(
        "t".to_string(),
        vec![SchemaColumn::new(
            "k".to_string(),
            TypeFamily::I64,
            mudu_type::data_type_codec::from_opt_object(&DataType::default_for(TypeFamily::I64)),
        )],
        vec![0],
        vec![],
    );
    plain.set_schema(DEFAULT_SCHEMA.to_string());
    let table_id = plain.id();
    let x_contract = Arc::new(MockXContract::new(table_id, Vec::new()));
    let meta = Arc::new(MockMetaMgr::new(plain, None));
    let cmd = make_ack(x_contract, meta, table_id, 0);
    let err = block_on(cmd.prepare()).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::ObjectNotFound);
}

fn make_ack_for(
    x_contract: Arc<MockXContract>,
    meta: Arc<MockMetaMgr>,
    table_id: OID,
    seq: u64,
    consumer: &str,
) -> AckQueue {
    AckQueue::new(
        PAckQueue {
            tx_mgr: Arc::new(MockTxMgr),
            table_id,
            seq,
            consumer: Some(consumer.to_string()),
        },
        x_contract,
        meta,
    )
}

#[test]
fn ack_for_consumer_advances_frontier_and_gcs_all_acked_prefix() {
    let queue = queue_schema("q");
    let queue_id = queue.id();
    // All rows are ready: the fan-out receive never claims.
    let x_contract = Arc::new(
        MockXContract::new(
            queue_id,
            vec![
                queue_row(0, 100, QUEUE_STATUS_READY, 0, "a"),
                queue_row(1, 100, QUEUE_STATUS_READY, 0, "b"),
                queue_row(2, 100, QUEUE_STATUS_READY, 0, "c"),
                queue_row(3, 100, QUEUE_STATUS_READY, 0, "d"),
            ],
        )
        .with_consumer(queue_id, "cg1", 0)
        .with_consumer(queue_id, "cg2", 0),
    );
    let meta = Arc::new(MockMetaMgr::new(queue, None));
    // cg1 acks up to seq 2 (frontier 3); cg2 is still at 0, so nothing is
    // collected.
    let cmd = make_ack_for(x_contract.clone(), meta.clone(), queue_id, 2, "cg1");
    block_on(cmd.prepare()).unwrap();
    block_on(cmd.run()).unwrap();
    assert_eq!(block_on(cmd.affected_rows()).unwrap(), 0);
    assert_eq!(x_contract.consumer_frontier_of(queue_id, "cg1"), Some(3));
    assert_eq!(x_contract.rows_of(queue_id), vec![0, 1, 2, 3]);

    // cg2 acks up to seq 1 (frontier 2): the GC watermark is min(3, 2), so
    // rows 0 and 1 go.
    let cmd = make_ack_for(x_contract.clone(), meta, queue_id, 1, "cg2");
    block_on(cmd.prepare()).unwrap();
    block_on(cmd.run()).unwrap();
    assert_eq!(block_on(cmd.affected_rows()).unwrap(), 2);
    assert_eq!(x_contract.consumer_frontier_of(queue_id, "cg2"), Some(2));
    assert_eq!(x_contract.rows_of(queue_id), vec![2, 3]);
}

#[test]
fn ack_for_consumer_is_idempotent_and_never_moves_backwards() {
    let queue = queue_schema("q");
    let queue_id = queue.id();
    let x_contract = Arc::new(
        MockXContract::new(
            queue_id,
            vec![
                queue_row(0, 100, QUEUE_STATUS_READY, 0, "a"),
                queue_row(1, 100, QUEUE_STATUS_READY, 0, "b"),
            ],
        )
        .with_consumer(queue_id, "cg", 0),
    );
    let meta = Arc::new(MockMetaMgr::new(queue, None));
    let cmd = make_ack_for(x_contract.clone(), meta.clone(), queue_id, 1, "cg");
    block_on(cmd.prepare()).unwrap();
    block_on(cmd.run()).unwrap();
    assert_eq!(block_on(cmd.affected_rows()).unwrap(), 2);
    assert!(x_contract.rows_of(queue_id).is_empty());
    // A repeated or lower ack keeps the frontier and deletes nothing.
    let cmd = make_ack_for(x_contract.clone(), meta, queue_id, 0, "cg");
    block_on(cmd.run()).unwrap();
    assert_eq!(block_on(cmd.affected_rows()).unwrap(), 0);
    assert_eq!(x_contract.consumer_frontier_of(queue_id, "cg"), Some(2));
}

#[test]
fn ack_for_unknown_consumer_fails() {
    let queue = queue_schema("q");
    let queue_id = queue.id();
    let x_contract = Arc::new(MockXContract::new(queue_id, Vec::new()));
    let meta = Arc::new(MockMetaMgr::new(queue, None));
    let cmd = make_ack_for(x_contract, meta, queue_id, 0, "missing");
    let err = block_on(cmd.prepare()).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::ObjectNotFound);
    let err = block_on(cmd.run()).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::ObjectNotFound);
}

#[test]
fn ack_without_for_on_fanout_queue_fails_at_prepare() {
    let queue = queue_schema("q");
    let queue_id = queue.id();
    let x_contract =
        Arc::new(MockXContract::new(queue_id, Vec::new()).with_consumer(queue_id, "cg", 0));
    let meta = Arc::new(MockMetaMgr::new(queue, None));
    let cmd = make_ack(x_contract, meta, queue_id, 0);
    let err = block_on(cmd.prepare()).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::InvalidArgument);
}
