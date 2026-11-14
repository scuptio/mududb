//! Pin every mock's surface: implemented stubs return their fixed values,
//! `unimplemented!()` stubs panic (caught here), so a trait-method signature
//! drift breaks this test instead of a distant syscall test.
use super::*;
use ::catalog::contract::worker_snapshot::WorkerSnapshot;
use ::wal::xl_batch::XLBatch;
use ::xengine::tx_mgr::{PhysicalRelationId, TxMgr};
use mudu::channel::MessageKind as BusMessageKind;
use std::panic::{AssertUnwindSafe, catch_unwind};

fn expect_panic(f: impl FnOnce()) {
    assert!(catch_unwind(AssertUnwindSafe(f)).is_err());
}

/// Poll `fut` to completion inside a catch_unwind: the async mocks panic on
/// first poll.
fn expect_panic_async<Fut>(fut: Fut)
where
    Fut: std::future::Future + 'static,
    Fut::Output: 'static,
{
    expect_panic(move || {
        let _ = mudu_sys::task::async_::block_on_tokio_current_thread(fut).unwrap();
    });
}

/// A `TxMgr` whose required methods are all inert, used only as a call
/// argument: the mocks under test panic before touching it.
struct NullTxMgr;

impl NullTxMgr {
    fn arc() -> Arc<Self> {
        Arc::new(Self)
    }
}

#[async_trait]
impl TxMgr for NullTxMgr {
    fn xid(&self) -> u64 {
        0
    }
    fn snapshot(&self) -> WorkerSnapshot {
        WorkerSnapshot::new(0, vec![])
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
        vec![]
    }
    fn staged_relation_ops(
        &self,
    ) -> std::collections::BTreeMap<
        PhysicalRelationId,
        std::collections::BTreeMap<Vec<u8>, Option<Vec<u8>>>,
    > {
        Default::default()
    }
    fn staged_items_in_range(
        &self,
        _start_key: &[u8],
        _end_key: &[u8],
    ) -> Vec<(Vec<u8>, Option<Vec<u8>>)> {
        vec![]
    }
    fn staged_put_items(&self) -> std::collections::BTreeMap<Vec<u8>, Option<Vec<u8>>> {
        Default::default()
    }
    fn is_empty(&self) -> bool {
        true
    }
    fn write_ops(&self) -> Vec<(PhysicalRelationId, Vec<u8>)> {
        vec![]
    }
    fn build_write_ops(&self) {}
    fn xl_batch(&self) -> XLBatch {
        XLBatch::new(vec![])
    }
}

fn schema() -> SchemaTable {
    SchemaTable::new("t".to_string(), vec![], vec![], vec![])
}

fn datum() -> VecDatum {
    VecDatum::new(vec![])
}

#[test]
fn null_xcontract_methods_are_all_unimplemented() {
    expect_panic_async(async move {
        let _ = NullXContract
            .create_table(NullTxMgr::arc(), &schema())
            .await;
    });
    expect_panic_async(async move {
        let _ = NullXContract.drop_table(NullTxMgr::arc(), 1).await;
    });
    expect_panic_async(async move {
        let _ = NullXContract.begin_tx().await;
    });
    expect_panic_async(async move {
        let _ = NullXContract.commit_tx(NullTxMgr::arc()).await;
    });
    expect_panic_async(async move {
        let _ = NullXContract.abort_tx(NullTxMgr::arc()).await;
    });
    expect_panic_async(async move {
        let _ = NullXContract
            .update(
                NullTxMgr::arc(),
                1,
                &datum(),
                &Predicate::CNF(vec![]),
                &datum(),
                &OptUpdate::default(),
            )
            .await;
    });
    expect_panic_async(async move {
        let _ = NullXContract
            .read_key(
                NullTxMgr::arc(),
                1,
                &datum(),
                &VecSelTerm::new(vec![]),
                &OptRead::default(),
            )
            .await;
    });
    expect_panic_async(async move {
        let _ = NullXContract
            .read_range(
                NullTxMgr::arc(),
                1,
                &RangeData::new(std::ops::Bound::Unbounded, std::ops::Bound::Unbounded),
                &Predicate::CNF(vec![]),
                &VecSelTerm::new(vec![]),
                &OptRead::default(),
            )
            .await;
    });
    expect_panic_async(async move {
        let _ = NullXContract
            .delete(
                NullTxMgr::arc(),
                1,
                &datum(),
                &Predicate::CNF(vec![]),
                &OptDelete::default(),
            )
            .await;
    });
    expect_panic_async(async move {
        let _ = NullXContract
            .insert(
                NullTxMgr::arc(),
                1,
                &datum(),
                &datum(),
                &OptInsert::default(),
            )
            .await;
    });
}

#[test]
fn null_meta_mgr_methods_are_all_unimplemented() {
    expect_panic_async(async move {
        let _ = NullMetaMgr.initialize().await;
    });
    expect_panic_async(async move {
        let _ = NullMetaMgr.get_table_by_id(1).await;
    });
    expect_panic_async(async move {
        let _ = NullMetaMgr.get_table_by_name("s", "t").await;
    });
    expect_panic_async(async move {
        let _ = NullMetaMgr.create_table(&schema()).await;
    });
    expect_panic_async(async move {
        let _ = NullMetaMgr.drop_table(1).await;
    });
    expect_panic_async(async move {
        let _ = NullMetaMgr.list_schemas().await;
    });
}

#[test]
fn null_worker_channel_methods_are_all_unimplemented() {
    expect_panic(|| {
        let _ = NullWorkerChannel.local_endpoint();
    });
    expect_panic_async(async move {
        let _ = NullWorkerChannel
            .send(1, OutgoingMessage::new(BusMessageKind::User(1), vec![]))
            .await;
    });
    expect_panic_async(async move {
        let _ = NullWorkerChannel.recv(RecvFilter::default(), None).await;
    });
    expect_panic(|| {
        let _ = NullWorkerChannel.register_kind_handler(
            BusMessageKind::User(1),
            Arc::new(|_e| Box::pin(async move { Ok(()) })),
        );
    });
    expect_panic(|| {
        let _ = NullWorkerChannel.unregister_kind_handler(BusMessageKind::User(1));
    });
}

#[test]
fn worker_local_sql_methods_of_mocks_stay_unimplemented() {
    // FakeWorkerLocal's SQL surface is unimplemented: the relation/fs
    // variants inherit that behavior through their forwarding layer.
    expect_panic_async(async move {
        let _ = relation_worker_local()
            .query(1, None, Box::new("SELECT 1".to_string()), Box::new(()))
            .await;
    });
    expect_panic_async(async move {
        let _ = relation_worker_local()
            .execute(1, None, Box::new("SELECT 1".to_string()), Box::new(()))
            .await;
    });
    expect_panic_async(async move {
        let _ = relation_worker_local()
            .batch(1, None, Box::new("SELECT 1".to_string()), Box::new(()))
            .await;
    });
    expect_panic_async(async move {
        let _ = relation_worker_local()
            .execute_async(1, WorkerExecute::BeginTx)
            .await;
    });
    expect_panic_async(async move {
        let _ = fs_worker_local()
            .execute_async(1, WorkerExecute::CommitTx)
            .await;
    });
    expect_panic_async(async move {
        // Session-aware rejects unknown sessions before forwarding; mint a
        // live one so the call reaches the (unimplemented) base.
        let sa = session_aware_worker_local();
        let session = sa.open_async().await.unwrap();
        let _ = sa.execute_async(session, WorkerExecute::RollbackTx).await;
    });
}
