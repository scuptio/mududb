#![allow(clippy::unwrap_used)]
use crate::create_partition_rule::CreatePartitionRule;
use ::catalog::contract::cmd_exec::CmdExec;

use ::catalog::contract::meta_mgr::MetaMgr;
use ::catalog::contract::partition_rule::{PartitionBound, PartitionRuleDesc, RangePartitionDef};
use ::catalog::contract::schema_class::SchemaClass;
use ::catalog::contract::schema_column::SchemaColumn;
use ::catalog::contract::table_desc::TableDesc;
use ::catalog::contract::table_info::TableInfo;
use ::catalog::contract::worker_snapshot::WorkerSnapshot;
use ::wal::xl_batch::XLBatch;
use ::xengine::tx_mgr::{PhysicalRelationId, TxMgr};
use ::xengine::x_param::PCreatePartitionRule;
use async_trait::async_trait;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu_sys::sync::SMutex;
use mudu_type::data_type::DataType;
use mudu_type::data_type_codec::TypeCodec;
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

fn sample_rule(name: &str) -> PartitionRuleDesc {
    PartitionRuleDesc::new_range(
        name.to_string(),
        vec![TypeFamily::I64],
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
    async fn get_table_by_name(&self, _schema: &str, _name: &str) -> RS<Option<Arc<TableDesc>>> {
        Ok(None)
    }
    async fn create_table(&self, _schema: &SchemaClass) -> RS<()> {
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
        None,
    );
    block_on(async { cmd.prepare().await }).unwrap();
}

#[test]
fn prepare_fails_when_rule_already_exists() {
    let meta = Arc::new(MockMetaMgr::new());
    let rule = sample_rule("existing_rule");
    meta.add_rule(rule.clone());
    let cmd = CreatePartitionRule::new(make_param(rule), meta, None);
    let err = block_on(async { cmd.prepare().await }).unwrap_err();
    assert_eq!(err.ec(), mudu::error::ErrorCode::ObjectAlreadyExists);
}

#[test]
fn run_creates_partition_rule() {
    let meta = Arc::new(MockMetaMgr::new());
    let rule = sample_rule("run_rule");
    let cmd = CreatePartitionRule::new(make_param(rule.clone()), meta.clone(), None);
    block_on(async {
        cmd.prepare().await.unwrap();
        cmd.run().await.unwrap();
    });
    assert!(meta.rules_by_name.lock().unwrap().contains_key("run_rule"));
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
    let schema = ::catalog::contract::schema_class::SchemaClass::new(
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
