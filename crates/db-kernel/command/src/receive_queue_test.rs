#![allow(clippy::unwrap_used)]

use crate::receive_queue::ReceiveQueue;
use ::catalog::contract::app_schema::DEFAULT_SCHEMA;
use ::catalog::contract::meta_mgr::MetaMgr;
use ::catalog::contract::query_exec::QueryExec;
use ::catalog::contract::queue::{
    decode_queue_seq, decode_queue_status, queue_lsn_datum, queue_receive_count_datum,
    queue_seq_datum, queue_status_datum, QUEUE_RECEIVE_COUNT_ATTR, QUEUE_SEQ_ATTR,
    QUEUE_STATUS_ATTR, QUEUE_STATUS_CLAIMED, QUEUE_STATUS_READY,
};
use ::catalog::contract::schema_class::{SchemaClass, SchemaClassKind};
use ::catalog::contract::schema_column::SchemaColumn;
use ::catalog::contract::table_desc::TableDesc;
use ::catalog::contract::table_info::TableInfo;
use ::catalog::contract::worker_snapshot::WorkerSnapshot;
use ::wal::xl_batch::XLBatch;
use ::xengine::api::{
    OptDelete, OptInsert, OptRead, OptUpdate, Predicate, QueueWaiter as _, RSCursor, RangeData,
    TupleRow, VecDatum, VecSelTerm, XContract, XContractQueue,
};
use ::xengine::tx_mgr::{PhysicalRelationId, TxMgr};
use ::xengine::x_param::PReceiveQueue;
use async_trait::async_trait;
use mudu::common::buf::Buf;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu_sys::sync::SMutex;
use mudu_sys::time::instant_now;
use mudu_type::data_type::DataType;
use mudu_type::type_family::TypeFamily;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

fn block_on<F>(fut: F) -> F::Output
where
    F: std::future::Future,
{
    mudu_sys::task::async_::build_current_thread_runtime()
        .unwrap()
        .block_on(fut)
}

/// Builds the queue schema the binder produces: four system columns plus a
/// single `body` payload column.
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

fn string_from_datum(bin: &[u8]) -> String {
    use mudu_type::datum::DatumDyn;
    mudu_contract::tuple::typed_bin::TypedBin::new(TypeFamily::String, bin.to_vec())
        .to_value(&DataType::default_for(TypeFamily::String))
        .unwrap()
        .into_string()
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
        7
    }
    fn snapshot(&self) -> WorkerSnapshot {
        WorkerSnapshot::new(7, Vec::new())
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

/// In-memory queue table: full attribute vectors keyed by the decoded
/// `__seq`. `locked` simulates rows claimed by an in-flight concurrent
/// transaction (their update fails with a lock-conflict transaction error).
/// `consumers` holds the named consumer cursors (fan-out mode) as
/// `name -> frontier`.
#[derive(Default)]
struct MockStore {
    rows: BTreeMap<u64, Vec<Option<Buf>>>,
    locked: HashSet<u64>,
    consumers: HashMap<String, u64>,
}

struct MockXContract {
    store: SMutex<MockStore>,
    wait_notify: Arc<mudu_sys::sync::async_::ANotify>,
}

/// A never-auto-waking latch for the WAIT tests: the test drives wakeups
/// explicitly through the shared `ANotify`.
struct MockQueueWaiter {
    notify: Arc<mudu_sys::sync::async_::ANotify>,
}

#[async_trait]
impl ::xengine::api::QueueWaiter for MockQueueWaiter {
    fn clear_signal(&self) {
        self.notify.clear_signal();
    }

    async fn park(&self, timeout: Option<std::time::Duration>) {
        match timeout {
            Some(duration) => {
                let _ = mudu_sys::task::async_::timeout(duration, self.notify.notified()).await;
            }
            None => {
                self.notify.notified().await;
            }
        }
    }
}

impl MockXContract {
    fn with_rows(rows: Vec<Vec<Option<Buf>>>) -> Self {
        let mut table = BTreeMap::new();
        for row in rows {
            let seq = decode_queue_seq(row[0].as_ref().unwrap()).unwrap();
            table.insert(seq, row);
        }
        Self {
            store: SMutex::new(MockStore {
                rows: table,
                locked: HashSet::new(),
                consumers: HashMap::new(),
            }),
            wait_notify: Arc::new(mudu_sys::sync::async_::ANotify::new()),
        }
    }

    /// Registers a consumer cursor with the given frontier (fan-out mode).
    fn with_consumer(self, name: &str, frontier: u64) -> Self {
        self.store
            .lock()
            .unwrap()
            .consumers
            .insert(name.to_string(), frontier);
        self
    }

    fn lock_key(&self, seq: u64) {
        self.store.lock().unwrap().locked.insert(seq);
    }

    fn row(&self, seq: u64) -> Option<Vec<Option<Buf>>> {
        self.store.lock().unwrap().rows.get(&seq).cloned()
    }

    /// The mock hosts a single shard (index 0, partition 0) per queue.
    fn mock_shard(table_id: OID) -> ::xengine::api::QueueShard {
        ::xengine::api::QueueShard {
            queue_oid: table_id,
            shard_index: 0,
            partition_id: 0,
            worker_id: 0,
        }
    }

    /// Appends a row as if a concurrent enqueue committed it.
    fn push_row(&self, row: Vec<Option<Buf>>) {
        let seq = decode_queue_seq(row[0].as_ref().unwrap()).unwrap();
        self.store.lock().unwrap().rows.insert(seq, row);
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
        _table_id: OID,
        _pred_key: &RangeData,
        _pred_non_key: &Predicate,
        _select: &VecSelTerm,
        _opt_read: &OptRead,
    ) -> RS<Arc<dyn RSCursor>> {
        unimplemented!()
    }
    async fn delete(
        &self,
        _tx_mgr: Arc<dyn TxMgr>,
        _table_id: OID,
        _pred_key: &VecDatum,
        _pred_non_key: &Predicate,
        _opt_delete: &OptDelete,
    ) -> RS<usize> {
        unimplemented!()
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
}

#[async_trait]
impl XContractQueue for MockXContract {
    async fn queue_shards(&self, table_id: OID) -> RS<Vec<::xengine::api::QueueShard>> {
        Ok(vec![Self::mock_shard(table_id)])
    }

    async fn queue_prepare_shard(&self, _shard: &::xengine::api::QueueShard) -> RS<()> {
        Ok(())
    }

    async fn consumers_exist_in_tx(&self, _tx: Arc<dyn TxMgr>, _queue_oid: OID) -> RS<bool> {
        Ok(!self.store.lock().unwrap().consumers.is_empty())
    }

    async fn consumer_frontier_in_tx(
        &self,
        _tx: Arc<dyn TxMgr>,
        _shard: &::xengine::api::QueueShard,
        consumer: &str,
    ) -> RS<Option<u64>> {
        Ok(self.store.lock().unwrap().consumers.get(consumer).copied())
    }

    async fn queue_claim_round(
        &self,
        _tx: Arc<dyn TxMgr>,
        _shard: &::xengine::api::QueueShard,
        count: u64,
    ) -> RS<Vec<::xengine::api::QueueClaimedRow>> {
        let mut store = self.store.lock().unwrap();
        let mut targets = Vec::new();
        for (seq, row) in store.rows.iter() {
            if targets.len() as u64 >= count {
                break;
            }
            let status = decode_queue_status(row[QUEUE_STATUS_ATTR].as_ref().unwrap()).unwrap();
            if status != QUEUE_STATUS_READY || store.locked.contains(seq) {
                continue;
            }
            targets.push(*seq);
        }
        let mut claimed = Vec::new();
        for seq in targets {
            let row = store.rows.get_mut(&seq).unwrap();
            row[QUEUE_STATUS_ATTR] = Some(queue_status_datum(QUEUE_STATUS_CLAIMED).unwrap());
            let rc = ::catalog::contract::queue::decode_queue_receive_count(
                row[QUEUE_RECEIVE_COUNT_ATTR].as_ref().unwrap(),
            )
            .unwrap()
                + 1;
            row[QUEUE_RECEIVE_COUNT_ATTR] = Some(queue_receive_count_datum(rc).unwrap());
            claimed.push(::xengine::api::QueueClaimedRow {
                seq,
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
            .consumer_frontier_in_tx(Arc::new(MockTxMgr), shard, consumer)
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

    async fn queue_receive_wait(
        &self,
        shard: &::xengine::api::QueueShard,
        count: u64,
        wait: ::catalog::contract::queue::QueueWait,
        consumer: Option<String>,
    ) -> RS<Vec<Vec<Option<Buf>>>> {
        let deadline = match wait {
            ::catalog::contract::queue::QueueWait::Timeout(ms) => {
                Some(instant_now() + std::time::Duration::from_millis(ms))
            }
            ::catalog::contract::queue::QueueWait::Forever => None,
        };
        let waiter = MockQueueWaiter {
            notify: self.wait_notify.clone(),
        };
        let mut rows = Vec::new();
        let mut scan_from = 0u64;
        loop {
            waiter.clear_signal();
            match &consumer {
                Some(name) => {
                    let frontier = self
                        .consumer_frontier_in_tx(Arc::new(MockTxMgr), shard, name)
                        .await?
                        .ok_or_else(|| {
                            mudu::mudu_error!(ErrorCode::ObjectNotFound, "no such consumer")
                        })?;
                    let base = frontier.max(scan_from);
                    let store = self.store.lock().unwrap();
                    let mut round: Vec<Vec<Option<Buf>>> = store
                        .rows
                        .iter()
                        .filter(|(seq, _)| **seq >= base)
                        .take(count as usize - rows.len())
                        .map(|(_, row)| row.clone())
                        .collect();
                    if let Some(last) = round.last() {
                        scan_from =
                            decode_queue_seq(last[QUEUE_SEQ_ATTR].as_ref().unwrap()).unwrap() + 1;
                    }
                    rows.append(&mut round);
                }
                None => {
                    let claimed = self
                        .queue_claim_round(Arc::new(MockTxMgr), shard, count - rows.len() as u64)
                        .await?;
                    rows.extend(claimed.into_iter().map(|row| row.fields));
                }
            }
            if !rows.is_empty() || count == 0 {
                return Ok(rows);
            }
            let remaining = match deadline {
                Some(deadline) => {
                    let now = instant_now();
                    if now >= deadline {
                        return Ok(rows);
                    }
                    Some(deadline - now)
                }
                None => None,
            };
            waiter.park(remaining).await;
        }
    }
}

struct MockMetaMgr {
    table: Arc<TableDesc>,
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

fn make_receive(
    x_contract: Arc<MockXContract>,
    meta: Arc<MockMetaMgr>,
    count: u64,
) -> ReceiveQueue {
    make_receive_with_wait(x_contract, meta, count, None)
}

fn make_receive_with_wait(
    x_contract: Arc<MockXContract>,
    meta: Arc<MockMetaMgr>,
    count: u64,
    wait: Option<::catalog::contract::queue::QueueWait>,
) -> ReceiveQueue {
    make_receive_full(x_contract, meta, count, wait, None)
}

fn make_receive_full(
    x_contract: Arc<MockXContract>,
    meta: Arc<MockMetaMgr>,
    count: u64,
    wait: Option<::catalog::contract::queue::QueueWait>,
    consumer: Option<String>,
) -> ReceiveQueue {
    let desc = meta.table.clone();
    ReceiveQueue::new(
        PReceiveQueue {
            tx_mgr: Arc::new(MockTxMgr),
            table_id: desc.id(),
            count,
            wait,
            consumer,
            tuple_desc: ::catalog::contract::queue::queue_envelope_tuple_desc(desc.as_ref()),
        },
        x_contract,
        meta,
    )
}

fn collect_rows(exec: &ReceiveQueue) -> Vec<TupleRow> {
    block_on(exec.open()).unwrap();
    let mut rows = Vec::new();
    while let Some(row) = block_on(exec.next()).unwrap() {
        rows.push(row);
    }
    rows
}

#[test]
fn receive_claims_ready_rows_and_returns_envelopes() {
    let x_contract = Arc::new(MockXContract::with_rows(vec![
        queue_row(0, 100, QUEUE_STATUS_READY, 0, "a"),
        queue_row(1, 100, QUEUE_STATUS_READY, 0, "b"),
        queue_row(2, 100, QUEUE_STATUS_READY, 0, "c"),
    ]));
    let schema = queue_schema("q");
    let meta = Arc::new(MockMetaMgr {
        table: TableInfo::new(schema).unwrap().table_desc().unwrap(),
    });
    let exec = make_receive(x_contract.clone(), meta, 2);

    let desc = exec.tuple_desc().unwrap();
    let names: Vec<&str> = desc.fields().iter().map(|f| f.name()).collect();
    assert_eq!(names, vec!["__seq", "__lsn", "body"]);

    let rows = collect_rows(&exec);
    assert_eq!(rows.len(), 2);
    let row0 = rows[0].fields();
    assert_eq!(decode_queue_seq(row0[0].as_ref().unwrap()).unwrap(), 0);
    assert_eq!(
        string_from_datum(row0[2].as_ref().unwrap()),
        "a".to_string()
    );
    let row1 = rows[1].fields();
    assert_eq!(decode_queue_seq(row1[0].as_ref().unwrap()).unwrap(), 1);

    // The claim updated status to claimed and incremented receive_count.
    let claimed = x_contract.row(0).unwrap();
    assert_eq!(
        decode_queue_status(claimed[2].as_ref().unwrap()).unwrap(),
        QUEUE_STATUS_CLAIMED
    );
    assert_eq!(
        ::catalog::contract::queue::decode_queue_receive_count(claimed[3].as_ref().unwrap())
            .unwrap(),
        1
    );
    // The third row was not claimed.
    let untouched = x_contract.row(2).unwrap();
    assert_eq!(
        decode_queue_status(untouched[2].as_ref().unwrap()).unwrap(),
        QUEUE_STATUS_READY
    );
}

#[test]
fn receive_skips_already_claimed_rows() {
    let x_contract = Arc::new(MockXContract::with_rows(vec![
        queue_row(0, 100, QUEUE_STATUS_CLAIMED, 1, "a"),
        queue_row(1, 100, QUEUE_STATUS_READY, 0, "b"),
        queue_row(2, 100, QUEUE_STATUS_READY, 0, "c"),
    ]));
    let schema = queue_schema("q");
    let meta = Arc::new(MockMetaMgr {
        table: TableInfo::new(schema).unwrap().table_desc().unwrap(),
    });
    let exec = make_receive(x_contract, meta, 2);
    let rows = collect_rows(&exec);
    let seqs: Vec<u64> = rows
        .iter()
        .map(|row| decode_queue_seq(row.fields()[0].as_ref().unwrap()).unwrap())
        .collect();
    assert_eq!(seqs, vec![1, 2]);
}

#[test]
fn receive_skips_rows_locked_by_an_in_flight_transaction() {
    let x_contract = Arc::new(MockXContract::with_rows(vec![
        queue_row(0, 100, QUEUE_STATUS_READY, 0, "a"),
        queue_row(1, 100, QUEUE_STATUS_READY, 0, "b"),
        queue_row(2, 100, QUEUE_STATUS_READY, 0, "c"),
    ]));
    x_contract.lock_key(1);
    let schema = queue_schema("q");
    let meta = Arc::new(MockMetaMgr {
        table: TableInfo::new(schema).unwrap().table_desc().unwrap(),
    });
    let exec = make_receive(x_contract, meta, 2);
    let rows = collect_rows(&exec);
    let seqs: Vec<u64> = rows
        .iter()
        .map(|row| decode_queue_seq(row.fields()[0].as_ref().unwrap()).unwrap())
        .collect();
    assert_eq!(seqs, vec![0, 2], "the locked row is skipped, not fatal");
}

#[test]
fn receive_from_empty_queue_returns_no_rows() {
    let x_contract = Arc::new(MockXContract::with_rows(Vec::new()));
    let schema = queue_schema("q");
    let meta = Arc::new(MockMetaMgr {
        table: TableInfo::new(schema).unwrap().table_desc().unwrap(),
    });
    let exec = make_receive(x_contract, meta, 4);
    assert!(collect_rows(&exec).is_empty());
}

#[test]
fn receive_wait_timeout_returns_empty_batch() {
    let x_contract = Arc::new(MockXContract::with_rows(Vec::new()));
    let schema = queue_schema("q");
    let meta = Arc::new(MockMetaMgr {
        table: TableInfo::new(schema).unwrap().table_desc().unwrap(),
    });
    let exec = make_receive_with_wait(
        x_contract,
        meta,
        2,
        Some(::catalog::contract::queue::QueueWait::Timeout(30)),
    );
    let started = mudu_sys::time::instant_now();
    let rows = collect_rows(&exec);
    assert!(rows.is_empty());
    assert!(
        started.elapsed() >= std::time::Duration::from_millis(20),
        "the wait parked until its timeout"
    );
}

#[test]
fn receive_wait_forever_returns_available_partial_batch() {
    let x_contract = Arc::new(MockXContract::with_rows(vec![queue_row(
        0,
        100,
        QUEUE_STATUS_READY,
        0,
        "a",
    )]));
    let schema = queue_schema("q");
    let meta = Arc::new(MockMetaMgr {
        table: TableInfo::new(schema).unwrap().table_desc().unwrap(),
    });
    let exec = make_receive_with_wait(
        x_contract,
        meta,
        3,
        Some(::catalog::contract::queue::QueueWait::Forever),
    );
    let rows = block_on(async {
        mudu_sys::task::async_::timeout(std::time::Duration::from_secs(1), async {
            exec.open().await.unwrap();
            let mut rows = Vec::new();
            while let Some(row) = exec.next().await.unwrap() {
                rows.push(row);
            }
            rows
        })
        .await
        .expect("partial batch must not park")
    });
    assert_eq!(
        rows.len(),
        1,
        "WAIT FOREVER returns the available row without filling the batch"
    );
}

#[test]
fn receive_wait_rescans_and_tops_up_on_wake() {
    block_on(async {
        let x_contract = Arc::new(MockXContract::with_rows(Vec::new()));
        let notify = x_contract.wait_notify.clone();
        let store = x_contract.clone();
        let schema = queue_schema("q");
        let meta = Arc::new(MockMetaMgr {
            table: TableInfo::new(schema).unwrap().table_desc().unwrap(),
        });
        let exec = make_receive_with_wait(
            x_contract,
            meta,
            1,
            Some(::catalog::contract::queue::QueueWait::Timeout(5000)),
        );
        let receiver = mudu_sys::task::async_::spawn_task_detached("receive-wait", async move {
            let mut rows = Vec::new();
            exec.open().await.unwrap();
            while let Some(row) = exec.next().await.unwrap() {
                rows.push(row);
            }
            rows
        })
        .unwrap();
        // Let the receiver run its first (empty) scan and park, then a
        // concurrent enqueue commits and wakes it.
        mudu_sys::task::async_::sleep(std::time::Duration::from_millis(20))
            .await
            .unwrap();
        store.push_row(queue_row(0, 100, QUEUE_STATUS_READY, 0, "woken"));
        notify.notify_waiters();
        let rows = receiver.await.unwrap().expect("receiver task output");
        assert_eq!(rows.len(), 1);
        assert_eq!(
            string_from_datum(rows[0].fields()[2].as_ref().unwrap()),
            "woken".to_string()
        );
        assert_eq!(
            decode_queue_status(store.row(0).unwrap()[2].as_ref().unwrap()).unwrap(),
            QUEUE_STATUS_CLAIMED
        );
    })
}

#[test]
fn receive_for_consumer_scans_past_frontier_without_claiming() {
    let x_contract = Arc::new(
        MockXContract::with_rows(vec![
            queue_row(0, 100, QUEUE_STATUS_READY, 0, "a"),
            queue_row(1, 100, QUEUE_STATUS_READY, 0, "b"),
            queue_row(2, 100, QUEUE_STATUS_READY, 0, "c"),
        ])
        .with_consumer("cg", 1),
    );
    let schema = queue_schema("q");
    let meta = Arc::new(MockMetaMgr {
        table: TableInfo::new(schema).unwrap().table_desc().unwrap(),
    });
    let exec = make_receive_full(x_contract.clone(), meta, 10, None, Some("cg".to_string()));
    let rows = collect_rows(&exec);
    let seqs: Vec<u64> = rows
        .iter()
        .map(|row| decode_queue_seq(row.fields()[0].as_ref().unwrap()).unwrap())
        .collect();
    assert_eq!(seqs, vec![1, 2], "the scan starts at the cursor frontier");
    // The fan-out scan claims nothing: status and receive count stay put.
    for seq in [0, 1, 2] {
        let row = x_contract.row(seq).unwrap();
        assert_eq!(
            decode_queue_status(row[2].as_ref().unwrap()).unwrap(),
            QUEUE_STATUS_READY
        );
        assert_eq!(
            ::catalog::contract::queue::decode_queue_receive_count(row[3].as_ref().unwrap())
                .unwrap(),
            0
        );
    }
}

#[test]
fn receive_for_consumer_caps_at_count() {
    let x_contract = Arc::new(
        MockXContract::with_rows(vec![
            queue_row(0, 100, QUEUE_STATUS_READY, 0, "a"),
            queue_row(1, 100, QUEUE_STATUS_READY, 0, "b"),
            queue_row(2, 100, QUEUE_STATUS_READY, 0, "c"),
        ])
        .with_consumer("cg", 0),
    );
    let schema = queue_schema("q");
    let meta = Arc::new(MockMetaMgr {
        table: TableInfo::new(schema).unwrap().table_desc().unwrap(),
    });
    let exec = make_receive_full(x_contract, meta, 2, None, Some("cg".to_string()));
    let rows = collect_rows(&exec);
    assert_eq!(rows.len(), 2);
}

#[test]
fn receive_for_unknown_consumer_fails() {
    let x_contract = Arc::new(MockXContract::with_rows(vec![queue_row(
        0,
        100,
        QUEUE_STATUS_READY,
        0,
        "a",
    )]));
    let schema = queue_schema("q");
    let meta = Arc::new(MockMetaMgr {
        table: TableInfo::new(schema).unwrap().table_desc().unwrap(),
    });
    let exec = make_receive_full(x_contract, meta, 1, None, Some("missing".to_string()));
    let err = block_on(exec.open()).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::ObjectNotFound);
}

#[test]
fn receive_without_for_on_fanout_queue_fails() {
    let x_contract = Arc::new(
        MockXContract::with_rows(vec![queue_row(0, 100, QUEUE_STATUS_READY, 0, "a")])
            .with_consumer("cg", 0),
    );
    let schema = queue_schema("q");
    let meta = Arc::new(MockMetaMgr {
        table: TableInfo::new(schema).unwrap().table_desc().unwrap(),
    });
    let exec = make_receive(x_contract, meta, 1);
    let err = block_on(exec.open()).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::InvalidArgument);
}

#[test]
fn receive_for_consumer_wait_forever_returns_available_partial_batch() {
    block_on(async {
        let store = Arc::new(
            MockXContract::with_rows(vec![queue_row(0, 100, QUEUE_STATUS_READY, 0, "a")])
                .with_consumer("cg", 0),
        );
        let meta = Arc::new(MockMetaMgr {
            table: TableInfo::new(queue_schema("q"))
                .unwrap()
                .table_desc()
                .unwrap(),
        });
        let exec = make_receive_full(
            store.clone(),
            meta,
            10,
            Some(::catalog::contract::queue::QueueWait::Forever),
            Some("cg".to_string()),
        );
        mudu_sys::task::async_::timeout(std::time::Duration::from_secs(1), exec.open())
            .await
            .expect("fan-out partial batch must not park")
            .unwrap();
        assert!(exec.next().await.unwrap().is_some());
        assert!(exec.next().await.unwrap().is_none());
        assert_eq!(
            decode_queue_status(store.row(0).unwrap()[2].as_ref().unwrap()).unwrap(),
            QUEUE_STATUS_READY
        );
    })
}
