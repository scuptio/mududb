#![allow(clippy::unwrap_used)]
use crate::command::create_partition_placement::CreatePartitionPlacement;
use crate::contract::cmd_exec::CmdExec;

use crate::contract::meta_mgr::MetaMgr;
use crate::contract::partition_rule_binding::PartitionPlacement;
use crate::contract::schema_column::SchemaColumn;
use crate::contract::schema_table::SchemaTable;
use crate::contract::table_desc::TableDesc;
use crate::contract::table_info::TableInfo;
use crate::server::worker_snapshot::WorkerSnapshot;
use crate::wal::xl_batch::XLBatch;
use crate::x_engine::tx_mgr::{PhysicalRelationId, TxMgr};
use crate::x_engine::x_param::PCreatePartitionPlacement;
use async_trait::async_trait;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu_sys::sync::SMutex;
use mudu_type::dat_type::DatType;
use mudu_type::dat_type_id::DatTypeID;
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
            DatTypeID::I64,
            DatType::new_no_param(DatTypeID::I64).to_info(),
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
    async fn get_table_by_name(&self, _name: &str) -> RS<Option<Arc<TableDesc>>> {
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
