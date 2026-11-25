#![allow(clippy::unwrap_used)]
use crate::insert_key_value::InsertKeyValue;
use ::catalog::contract::cmd_exec::CmdExec;

use ::catalog::contract::meta_mgr::MetaMgr;
use ::catalog::contract::schema_class::SchemaClass;
use ::catalog::contract::schema_column::SchemaColumn;
use ::catalog::contract::table_desc::TableDesc;
use ::catalog::contract::table_info::TableInfo;
use ::catalog::contract::worker_snapshot::WorkerSnapshot;
use ::wal::xl_batch::XLBatch;
use ::xengine::api::{
    OptDelete, OptInsert, OptRead, OptUpdate, Predicate, RSCursor, RangeData, VecDatum, VecSelTerm,
    XContract, XContractQueue,
};
use ::xengine::tx_mgr::{PhysicalRelationId, TxMgr};
use ::xengine::x_param::PInsertKeyValue;
use async_trait::async_trait;
use mudu::common::buf::Buf;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu_type::data_type::DataType;
use mudu_type::data_type_codec::TypeCodec;
use mudu_type::type_family::TypeFamily;
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
    let schema = SchemaClass::new(
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
    async fn create_table(&self, _tx_mgr: Arc<dyn TxMgr>, _schema: &SchemaClass) -> RS<()> {
        Ok(())
    }
    async fn drop_table(&self, _tx_mgr: Arc<dyn TxMgr>, _oid: OID) -> RS<()> {
        Ok(())
    }
    async fn alter_table(
        &self,
        _tx_mgr: Arc<dyn TxMgr>,
        _oid: OID,
        _alter_table: &::xengine::api::AlterTable,
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

impl XContractQueue for MockXContract {}

struct MockMetaMgr {
    table: Arc<TableDesc>,
}

impl MockMetaMgr {
    fn new(table: Arc<TableDesc>) -> Self {
        Self { table }
    }
}

#[async_trait]
impl MetaMgr for MockMetaMgr {
    async fn initialize(&self) -> RS<()> {
        Ok(())
    }
    async fn get_table_by_id(&self, _oid: OID) -> RS<Arc<TableDesc>> {
        Ok(self.table.clone())
    }
    async fn get_table_by_name(&self, _schema: &str, _name: &str) -> RS<Option<Arc<TableDesc>>> {
        Ok(None)
    }
    async fn create_table(&self, _schema: &SchemaClass) -> RS<()> {
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
        Arc::new(MockMetaMgr::new(dummy_table_desc())),
    );
    let err = block_on(async { cmd.prepare().await }).unwrap_err();
    assert_eq!(err.ec(), mudu::error::ErrorCode::InvalidTuple);
    assert!(
        err.message().contains("insert key is empty"),
        "unexpected message: {}",
        err.message()
    );
}

#[test]
fn prepare_fails_for_table_without_primary_key() {
    // A keyless table binds every insert row to an empty key; the error must
    // name the real cause (no primary key), not a lookup failure.
    let keyless = {
        let schema = SchemaClass::new(
            "log".to_string(),
            vec![SchemaColumn::new(
                "msg".to_string(),
                TypeFamily::String,
                DataType::default_for(TypeFamily::String).to_info(),
            )],
            vec![],
            vec![0],
        );
        TableInfo::new(schema).unwrap().table_desc().unwrap()
    };
    let cmd = InsertKeyValue::new(
        make_param(vec![(
            VecDatum::new(Vec::new()),
            VecDatum::new(vec![(0, datum(1))]),
        )]),
        Arc::new(MockXContract::new()),
        Arc::new(MockMetaMgr::new(keyless)),
    );
    let err = block_on(async { cmd.prepare().await }).unwrap_err();
    assert_eq!(err.ec(), mudu::error::ErrorCode::InvalidTuple);
    assert!(
        err.message().contains("without primary key"),
        "unexpected message: {}",
        err.message()
    );
}

#[test]
fn prepare_succeeds_for_non_empty_key() {
    let cmd = InsertKeyValue::new(
        make_param(vec![(
            VecDatum::new(vec![(0, datum(1))]),
            VecDatum::new(vec![(0, datum(2))]),
        )]),
        Arc::new(MockXContract::new()),
        Arc::new(MockMetaMgr::new(dummy_table_desc())),
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
        Arc::new(MockMetaMgr::new(dummy_table_desc())),
    );
    block_on(async {
        cmd.prepare().await.unwrap();
        cmd.run().await.unwrap();
        assert_eq!(cmd.affected_rows().await.unwrap(), 3);
    });
    assert_eq!(x_contract.insert_count.load(Ordering::Relaxed), 3);
}

// ---- Queue enqueue hook tests ----

use ::catalog::contract::queue::{
    decode_queue_lsn, decode_queue_receive_count, decode_queue_seq, decode_queue_status,
    queue_system_columns, QUEUE_STATUS_READY,
};
use ::catalog::contract::schema_class::SchemaClassKind;
use mudu_sys::sync::SMutex;
use std::collections::HashMap;

fn queue_table_desc() -> Arc<TableDesc> {
    let mut columns = queue_system_columns();
    columns.push(SchemaColumn::new(
        "body".to_string(),
        TypeFamily::String,
        DataType::default_for(TypeFamily::String).to_info(),
    ));
    let value_indices = (1..columns.len()).collect();
    let mut schema = SchemaClass::new("q".to_string(), columns, vec![0], value_indices);
    schema.set_kind(SchemaClassKind::Queue);
    TableInfo::new(schema).unwrap().table_desc().unwrap()
}

fn string_datum(value: &str) -> Buf {
    mudu_type::data_type_function::send_binary(
        &mudu_type::data_value::DataValue::from_string(value.to_string()),
        &DataType::default_for(TypeFamily::String),
    )
    .unwrap()
}

/// Structs inserted key/value datums and implements the queue sequence
/// allocator with a configurable depth cap.
type InsertedRows = Vec<(Vec<(usize, Buf)>, Vec<(usize, Buf)>)>;

struct QueueMockXContract {
    inserts: SMutex<InsertedRows>,
    next_seq: SMutex<u64>,
    max_depth: u64,
}

impl QueueMockXContract {
    fn new(max_depth: u64) -> Self {
        Self {
            inserts: SMutex::new(Vec::new()),
            next_seq: SMutex::new(0),
            max_depth,
        }
    }

    fn inserted(&self) -> InsertedRows {
        self.inserts.lock().unwrap().clone()
    }
}

#[async_trait]
impl XContract for QueueMockXContract {
    async fn create_table(&self, _tx_mgr: Arc<dyn TxMgr>, _schema: &SchemaClass) -> RS<()> {
        Ok(())
    }
    async fn drop_table(&self, _tx_mgr: Arc<dyn TxMgr>, _oid: OID) -> RS<()> {
        Ok(())
    }
    async fn alter_table(
        &self,
        _tx_mgr: Arc<dyn TxMgr>,
        _oid: OID,
        _alter_table: &::xengine::api::AlterTable,
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
        keys: &VecDatum,
        values: &VecDatum,
        _opt_insert: &OptInsert,
    ) -> RS<()> {
        self.inserts
            .lock()
            .unwrap()
            .push((keys.data().clone(), values.data().clone()));
        Ok(())
    }

    async fn insert_on_partition(
        &self,
        tx: Arc<dyn TxMgr>,
        table_id: OID,
        _partition_id: OID,
        key: &VecDatum,
        value: &VecDatum,
    ) -> RS<()> {
        self.insert(tx, table_id, key, value, &OptInsert::default())
            .await
    }
}

#[async_trait]
impl XContractQueue for QueueMockXContract {
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

    async fn queue_reserve(
        &self,
        _tx: Arc<dyn TxMgr>,
        _shard: &::xengine::api::QueueShard,
        count: u64,
        _bytes: u64,
    ) -> RS<u64> {
        let mut next = self.next_seq.lock().unwrap();
        if *next + count > self.max_depth {
            return Err(mudu::mudu_error!(
                mudu::error::ErrorCode::QueueFull,
                "queue depth cap reached"
            ));
        }
        let first = *next;
        *next += count;
        Ok(first)
    }
}

fn enqueue(rows: Vec<(VecDatum, VecDatum)>, x_contract: Arc<QueueMockXContract>) -> InsertKeyValue {
    InsertKeyValue::new(
        make_param(rows),
        x_contract,
        Arc::new(MockMetaMgr::new(queue_table_desc())),
    )
}

#[test]
fn enqueue_synthesizes_queue_system_columns() {
    let x_contract = Arc::new(QueueMockXContract::new(100));
    let cmd = enqueue(
        vec![(
            VecDatum::new(Vec::new()),
            VecDatum::new(vec![(6, string_datum("hello"))]),
        )],
        x_contract.clone(),
    );
    block_on(async {
        // The user row carries no key: the queue path accepts it and
        // synthesizes `__seq`.
        cmd.prepare().await.unwrap();
        cmd.run().await.unwrap();
        assert_eq!(cmd.affected_rows().await.unwrap(), 1);
    });
    let inserted = x_contract.inserted();
    assert_eq!(inserted.len(), 1);
    let (key, value) = &inserted[0];
    assert_eq!(key.len(), 1);
    assert_eq!(key[0].0, 0);
    assert_eq!(decode_queue_seq(&key[0].1).unwrap(), 0);
    let by_attr: HashMap<usize, Buf> = value.iter().cloned().collect();
    // __lsn is the enclosing transaction's xid (MockTxMgr returns 1).
    assert_eq!(decode_queue_lsn(&by_attr[&1]).unwrap(), 1);
    assert_eq!(
        decode_queue_status(&by_attr[&2]).unwrap(),
        QUEUE_STATUS_READY
    );
    assert_eq!(decode_queue_receive_count(&by_attr[&3]).unwrap(), 0);
    assert_eq!(by_attr[&6], string_datum("hello"));
}

#[test]
fn enqueue_allocates_monotonic_sequences() {
    let x_contract = Arc::new(QueueMockXContract::new(100));
    for expected in 0..3u64 {
        let cmd = enqueue(
            vec![(
                VecDatum::new(Vec::new()),
                VecDatum::new(vec![(6, string_datum("x"))]),
            )],
            x_contract.clone(),
        );
        block_on(cmd.run()).unwrap();
        let inserted = x_contract.inserted();
        let seq = decode_queue_seq(&inserted.last().unwrap().0[0].1).unwrap();
        assert_eq!(seq, expected);
    }
    // A multi-row statement allocates a consecutive batch.
    let cmd = enqueue(
        vec![
            (
                VecDatum::new(Vec::new()),
                VecDatum::new(vec![(6, string_datum("a"))]),
            ),
            (
                VecDatum::new(Vec::new()),
                VecDatum::new(vec![(6, string_datum("b"))]),
            ),
        ],
        x_contract.clone(),
    );
    block_on(cmd.run()).unwrap();
    let inserted = x_contract.inserted();
    let seqs: Vec<u64> = inserted[3..]
        .iter()
        .map(|(key, _)| decode_queue_seq(&key[0].1).unwrap())
        .collect();
    assert_eq!(seqs, vec![3, 4]);
}

#[test]
fn enqueue_fails_with_queue_full_at_the_depth_cap() {
    let x_contract = Arc::new(QueueMockXContract::new(2));
    let cmd = enqueue(
        vec![(
            VecDatum::new(Vec::new()),
            VecDatum::new(vec![(6, string_datum("x"))]),
        )],
        x_contract.clone(),
    );
    block_on(cmd.run()).unwrap();
    let cmd = enqueue(
        vec![(
            VecDatum::new(Vec::new()),
            VecDatum::new(vec![(6, string_datum("y"))]),
        )],
        x_contract.clone(),
    );
    block_on(cmd.run()).unwrap();
    let cmd = enqueue(
        vec![(
            VecDatum::new(Vec::new()),
            VecDatum::new(vec![(6, string_datum("z"))]),
        )],
        x_contract,
    );
    let err = block_on(cmd.run()).unwrap_err();
    assert_eq!(err.ec(), mudu::error::ErrorCode::QueueFull);
}
