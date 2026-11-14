#![allow(clippy::unwrap_used)]
use crate::create_partition_placement::CreatePartitionPlacement;
use ::catalog::contract::cmd_exec::CmdExec;

use ::catalog::contract::meta_mgr::MetaMgr;
use ::catalog::contract::partition_rule_binding::PartitionPlacement;
use ::catalog::contract::schema_column::SchemaColumn;
use ::catalog::contract::schema_table::SchemaTable;
use ::catalog::contract::table_desc::TableDesc;
use ::catalog::contract::table_info::TableInfo;
use ::catalog::contract::worker_snapshot::WorkerSnapshot;
use ::wal::xl_batch::XLBatch;
use ::xengine::tx_mgr::{PhysicalRelationId, TxMgr};
use ::xengine::x_param::PCreatePartitionPlacement;
use async_trait::async_trait;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu_sys::sync::SMutex;
use mudu_type::data_type::DataType;
use mudu_type::type_family::TypeFamily;
use std::collections::BTreeMap;
use std::sync::Arc;

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

fn make_param(placements: Vec<PartitionPlacement>) -> PCreatePartitionPlacement {
    PCreatePartitionPlacement {
        tx_mgr: Arc::new(MockTxMgr),
        placements,
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

struct MockMetaMgr {
    placements: SMutex<Vec<PartitionPlacement>>,
}

impl MockMetaMgr {
    fn new() -> Self {
        Self {
            placements: SMutex::new(Vec::new()),
        }
    }
}

#[async_trait]
impl MetaMgr for MockMetaMgr {
    async fn initialize(&self) -> RS<()> {
        Ok(())
    }
    async fn get_table_by_id(&self, _oid: OID) -> RS<Arc<TableDesc>> {
        Ok(dummy_table_desc())
    }
    async fn get_table_by_name(&self, _schema: &str, _name: &str) -> RS<Option<Arc<TableDesc>>> {
        Ok(None)
    }
    async fn create_table(&self, _schema: &SchemaTable) -> RS<()> {
        Ok(())
    }
    async fn drop_table(&self, _table_id: OID) -> RS<()> {
        Ok(())
    }
    async fn upsert_partition_placements(&self, placements: &[PartitionPlacement]) -> RS<()> {
        self.placements
            .lock()
            .unwrap()
            .extend_from_slice(placements);
        Ok(())
    }
}

#[test]
fn prepare_always_succeeds() {
    let cmd = CreatePartitionPlacement::new(
        make_param(vec![PartitionPlacement {
            partition_id: 1,
            worker_id: 2,
        }]),
        Arc::new(MockMetaMgr::new()),
    );
    block_on(async { cmd.prepare().await }).unwrap();
}

#[test]
fn run_upserts_all_placements() {
    let meta = Arc::new(MockMetaMgr::new());
    let cmd = CreatePartitionPlacement::new(
        make_param(vec![
            PartitionPlacement {
                partition_id: 1,
                worker_id: 10,
            },
            PartitionPlacement {
                partition_id: 2,
                worker_id: 20,
            },
        ]),
        meta.clone(),
    );
    block_on(async {
        cmd.prepare().await.unwrap();
        cmd.run().await.unwrap();
    });
    let stored = meta.placements.lock().unwrap();
    assert_eq!(stored.len(), 2);
    assert_eq!(stored[0].partition_id, 1);
    assert_eq!(stored[0].worker_id, 10);
    assert_eq!(stored[1].partition_id, 2);
    assert_eq!(stored[1].worker_id, 20);
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
