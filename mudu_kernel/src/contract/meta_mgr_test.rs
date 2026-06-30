#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use crate::contract::meta_mgr::MetaMgr;
use crate::contract::partition_rule::PartitionRuleDesc;
use crate::contract::partition_rule_binding::TablePartitionBinding;
use crate::contract::schema_column::SchemaColumn;
use crate::contract::schema_table::SchemaTable;
use crate::contract::table_desc::TableDesc;
use crate::contract::table_info::TableInfo;
use async_trait::async_trait;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_sys::sync::SMutex;
use mudu_type::dat_type::DatType;
use mudu_type::dat_type_id::DatTypeID;
use mudu_type::dt_info::DTInfo;
use std::collections::HashMap;
use std::sync::Arc;

struct StubMetaMgr {
    tables: SMutex<HashMap<OID, Arc<TableDesc>>>,
    tables_by_name: SMutex<HashMap<String, OID>>,
}

impl StubMetaMgr {
    fn new() -> Self {
        Self {
            tables: SMutex::new(HashMap::new()),
            tables_by_name: SMutex::new(HashMap::new()),
        }
    }
}

#[async_trait]
impl MetaMgr for StubMetaMgr {
    async fn initialize(&self) -> RS<()> {
        Ok(())
    }

    async fn get_table_by_id(&self, oid: OID) -> RS<Arc<TableDesc>> {
        self.tables.lock()?.get(&oid).cloned().ok_or_else(|| {
            mudu_error!(
                ErrorCode::EntityNotFound,
                format!("table {} not found", oid)
            )
        })
    }

    async fn get_table_by_name(&self, name: &str) -> RS<Option<Arc<TableDesc>>> {
        let oid = self.tables_by_name.lock()?.get(name).copied();
        match oid {
            Some(oid) => self.get_table_by_id(oid).await.map(Some),
            None => Ok(None),
        }
    }

    async fn create_table(&self, schema: &SchemaTable) -> RS<()> {
        let desc = TableInfo::new(schema.clone())?.table_desc()?;
        let mut tables = self.tables.lock()?;
        let mut by_name = self.tables_by_name.lock()?;
        tables.insert(desc.id(), desc.clone());
        by_name.insert(desc.name().clone(), desc.id());
        Ok(())
    }

    async fn drop_table(&self, table_id: OID) -> RS<()> {
        let desc = self.tables.lock()?.remove(&table_id).ok_or_else(|| {
            mudu_error!(
                ErrorCode::EntityNotFound,
                format!("table {} not found", table_id)
            )
        })?;
        self.tables_by_name.lock()?.remove(desc.name());
        Ok(())
    }
}

fn sample_schema(name: &str, oid: OID) -> SchemaTable {
    let column = SchemaColumn::new_with_oid(
        gen_oid(),
        "c1".to_string(),
        DatTypeID::I64,
        DTInfo::from_opt_object(&DatType::new_no_param(DatTypeID::I64)),
    );
    SchemaTable::new_with_oid(oid, name.to_string(), vec![column], vec![0], vec![])
}

fn gen_oid() -> OID {
    mudu_utils::oid::gen_oid()
}

#[test]
fn meta_mgr_create_partition_rule_returns_not_implemented() {
    let mgr = StubMetaMgr::new();
    let rule = PartitionRuleDesc::new_range("r".to_string(), vec![], vec![]);
    let err = futures::executor::block_on(mgr.create_partition_rule(&rule)).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::NotImplemented);
}

#[test]
fn meta_mgr_get_partition_rule_by_id_returns_entity_not_found_with_oid() {
    let mgr = StubMetaMgr::new();
    let oid = gen_oid();
    let err = futures::executor::block_on(mgr.get_partition_rule_by_id(oid)).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::EntityNotFound);
    assert!(err.message().contains(&oid.to_string()));
}

#[test]
fn meta_mgr_get_partition_rule_by_name_returns_none() {
    let mgr = StubMetaMgr::new();
    assert!(
        futures::executor::block_on(mgr.get_partition_rule_by_name("missing"))
            .unwrap()
            .is_none()
    );
}

#[test]
fn meta_mgr_list_partition_rules_returns_empty() {
    let mgr = StubMetaMgr::new();
    assert!(futures::executor::block_on(mgr.list_partition_rules())
        .unwrap()
        .is_empty());
}

#[test]
fn meta_mgr_bind_table_partition_returns_not_implemented() {
    let mgr = StubMetaMgr::new();
    let binding = TablePartitionBinding {
        table_id: gen_oid(),
        rule_id: gen_oid(),
        ref_attr_indices: vec![],
    };
    let err = futures::executor::block_on(mgr.bind_table_partition(&binding)).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::NotImplemented);
}

#[test]
fn meta_mgr_get_table_partition_binding_returns_none() {
    let mgr = StubMetaMgr::new();
    assert!(
        futures::executor::block_on(mgr.get_table_partition_binding(gen_oid()))
            .unwrap()
            .is_none()
    );
}

#[test]
fn meta_mgr_upsert_partition_placements_returns_not_implemented() {
    let mgr = StubMetaMgr::new();
    let err = futures::executor::block_on(mgr.upsert_partition_placements(&[])).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::NotImplemented);
}

#[test]
fn meta_mgr_get_partition_worker_returns_none() {
    let mgr = StubMetaMgr::new();
    assert!(
        futures::executor::block_on(mgr.get_partition_worker(gen_oid()))
            .unwrap()
            .is_none()
    );
}

#[test]
fn meta_mgr_list_partition_placements_returns_empty() {
    let mgr = StubMetaMgr::new();
    assert!(futures::executor::block_on(mgr.list_partition_placements())
        .unwrap()
        .is_empty());
}

#[test]
fn meta_mgr_initialize_succeeds() {
    let mgr = StubMetaMgr::new();
    futures::executor::block_on(mgr.initialize()).unwrap();
}

#[test]
fn meta_mgr_table_round_trip() {
    let mgr = StubMetaMgr::new();
    let oid = gen_oid();
    let schema = sample_schema("t1", oid);
    futures::executor::block_on(mgr.create_table(&schema)).unwrap();

    let by_id = futures::executor::block_on(mgr.get_table_by_id(oid)).unwrap();
    assert_eq!(by_id.id(), oid);
    assert_eq!(by_id.name(), "t1");

    let by_name = futures::executor::block_on(mgr.get_table_by_name("t1")).unwrap();
    assert!(by_name.is_some());
    assert_eq!(by_name.unwrap().id(), oid);
}

#[test]
fn meta_mgr_drop_table_removes_table() {
    let mgr = StubMetaMgr::new();
    let oid = gen_oid();
    let schema = sample_schema("t2", oid);
    futures::executor::block_on(mgr.create_table(&schema)).unwrap();
    futures::executor::block_on(mgr.drop_table(oid)).unwrap();

    assert!(futures::executor::block_on(mgr.get_table_by_id(oid)).is_err());
    assert!(futures::executor::block_on(mgr.get_table_by_name("t2"))
        .unwrap()
        .is_none());
}
