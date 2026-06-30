#![allow(clippy::unwrap_used)]
use crate::command::create_partition_rule::CreatePartitionRule;
use crate::contract::cmd_exec::CmdExec;

use crate::contract::meta_mgr::MetaMgr;
use crate::contract::partition_rule::{PartitionBound, PartitionRuleDesc, RangePartitionDef};
use crate::contract::schema_column::SchemaColumn;
use crate::contract::schema_table::SchemaTable;
use crate::contract::table_desc::TableDesc;
use crate::contract::table_info::TableInfo;
use crate::server::worker_snapshot::WorkerSnapshot;
use crate::wal::xl_batch::XLBatch;
use crate::x_engine::tx_mgr::{PhysicalRelationId, TxMgr};
use crate::x_engine::x_param::PCreatePartitionRule;
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

fn sample_rule(name: &str) -> PartitionRuleDesc {
    PartitionRuleDesc::new_range(
        name.to_string(),
        vec![DatTypeID::I64],
        vec![RangePartitionDef::new(
            "p0".to_string(),
            PartitionBound::Unbounded,
            PartitionBound::Unbounded,
        )],
    )
}

fn make_param(rule: PartitionRuleDesc) -> PCreatePartitionRule {
    PCreatePartitionRule {
        tx_mgr: Arc::new(MockTxMgr),
        rule,
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
    rules_by_name: SMutex<std::collections::HashMap<String, PartitionRuleDesc>>,
}

impl MockMetaMgr {
    fn new() -> Self {
        Self {
            rules_by_name: SMutex::new(std::collections::HashMap::new()),
        }
    }
    fn add_rule(&self, rule: PartitionRuleDesc) {
        self.rules_by_name
            .lock()
            .unwrap()
            .insert(rule.name.clone(), rule);
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
    async fn create_partition_rule(&self, rule: &PartitionRuleDesc) -> RS<()> {
        self.rules_by_name
            .lock()
            .unwrap()
            .insert(rule.name.clone(), rule.clone());
        Ok(())
    }
    async fn get_partition_rule_by_name(&self, name: &str) -> RS<Option<PartitionRuleDesc>> {
        Ok(self.rules_by_name.lock().unwrap().get(name).cloned())
    }
}

#[test]
fn prepare_succeeds_when_rule_does_not_exist() {
    let cmd = CreatePartitionRule::new(
        make_param(sample_rule("new_rule")),
        Arc::new(MockMetaMgr::new()),
    );
    block_on(async { cmd.prepare().await }).unwrap();
}

#[test]
fn prepare_fails_when_rule_already_exists() {
    let meta = Arc::new(MockMetaMgr::new());
    let rule = sample_rule("existing_rule");
    meta.add_rule(rule.clone());
    let cmd = CreatePartitionRule::new(make_param(rule), meta);
    let err = block_on(async { cmd.prepare().await }).unwrap_err();
    assert_eq!(err.ec(), mudu::error::ErrorCode::EntityAlreadyExists);
}

#[test]
fn run_creates_partition_rule() {
    let meta = Arc::new(MockMetaMgr::new());
    let rule = sample_rule("run_rule");
    let cmd = CreatePartitionRule::new(make_param(rule.clone()), meta.clone());
    block_on(async {
        cmd.prepare().await.unwrap();
        cmd.run().await.unwrap();
    });
    assert!(meta.rules_by_name.lock().unwrap().contains_key("run_rule"));
}
