#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

use super::Planner;
use crate::bound_stmt::{
    AggregateFunc, BoundAggregate, BoundPredicate, BoundQuery, BoundResidual, BoundSelect,
    BoundSelectColumn, BoundSelectItem,
};
use crate::plan_ctx::PlanCtx;
use ::catalog::contract::meta_mgr::MetaMgr;
use ::catalog::contract::schema_column::SchemaColumn;
use ::catalog::contract::schema_table::SchemaTable;
use ::catalog::contract::table_desc::TableDesc;
use ::catalog::contract::table_info::TableInfo;
use ::catalog::contract::worker_snapshot::WorkerSnapshot;
use ::xengine::api::{
    AlterTable, OptDelete, OptInsert, OptRead, OptUpdate, Predicate, RSCursor, RangeData, TupleRow,
    VecDatum, VecSelTerm, XContract,
};
use ::xengine::tx_mgr::{PhysicalRelationId, TxMgr};
use async_trait::async_trait;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu_contract::tuple::tuple_field_desc::TupleFieldDesc;
use mudu_contract::tuple::typed_bin::TypedBin;
use mudu_sys::sync::SMutex;
use mudu_type::data_type::DataType;
use mudu_type::data_type_info::DataTypeInfo;
use mudu_type::datum::DatumDyn;
use mudu_type::type_family::TypeFamily;
use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

struct TestMetaMgr {
    tables: SMutex<HashMap<OID, Arc<TableDesc>>>,
}

impl TestMetaMgr {
    fn new(schema: SchemaTable) -> Self {
        let table = TableInfo::new(schema).unwrap().table_desc().unwrap();
        let mut tables = HashMap::new();
        tables.insert(table.id(), table);
        Self {
            tables: SMutex::new(tables),
        }
    }

    fn table_id(&self) -> OID {
        *self.tables.lock().unwrap().keys().next().unwrap()
    }
}

#[async_trait]
impl MetaMgr for TestMetaMgr {
    async fn initialize(&self) -> RS<()> {
        Ok(())
    }

    async fn get_table_by_id(&self, oid: OID) -> RS<Arc<TableDesc>> {
        self.tables
            .lock()
            .unwrap()
            .get(&oid)
            .cloned()
            .ok_or_else(|| {
                mudu::mudu_error!(mudu::error::ErrorCode::EntityNotFound, oid.to_string())
            })
    }

    async fn get_table_by_name(&self, _schema: &str, name: &str) -> RS<Option<Arc<TableDesc>>> {
        Ok(self
            .tables
            .lock()
            .unwrap()
            .values()
            .find(|table| table.name() == name)
            .cloned())
    }

    async fn create_table(&self, schema: &SchemaTable) -> RS<()> {
        let table = TableInfo::new(schema.clone())?.table_desc()?;
        self.tables.lock().unwrap().insert(table.id(), table);
        Ok(())
    }

    async fn drop_table(&self, table_id: OID) -> RS<()> {
        self.tables.lock().unwrap().remove(&table_id);
        Ok(())
    }
}

struct TestTxMgr;

impl TxMgr for TestTxMgr {
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
    fn xl_batch(&self) -> ::wal::xl_batch::XLBatch {
        ::wal::xl_batch::XLBatch::new(Vec::new())
    }
}

struct TestCursor;

#[async_trait]
impl RSCursor for TestCursor {
    async fn next(&self) -> RS<Option<TupleRow>> {
        Ok(None)
    }
}

struct TestXContract {
    read_key_calls: AtomicUsize,
    read_range_calls: AtomicUsize,
}

impl TestXContract {
    fn new() -> Self {
        Self {
            read_key_calls: AtomicUsize::new(0),
            read_range_calls: AtomicUsize::new(0),
        }
    }
}

#[async_trait]
impl XContract for TestXContract {
    async fn create_table(&self, _tx_mgr: Arc<dyn TxMgr>, _schema: &SchemaTable) -> RS<()> {
        unimplemented!()
    }
    async fn drop_table(&self, _tx_mgr: Arc<dyn TxMgr>, _oid: OID) -> RS<()> {
        unimplemented!()
    }
    async fn alter_table(
        &self,
        _tx_mgr: Arc<dyn TxMgr>,
        _oid: OID,
        _alter_table: &AlterTable,
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
    ) -> RS<Option<Vec<Option<Vec<u8>>>>> {
        self.read_key_calls.fetch_add(1, Ordering::Relaxed);
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
        self.read_range_calls.fetch_add(1, Ordering::Relaxed);
        Ok(Arc::new(TestCursor))
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

fn composite_schema() -> SchemaTable {
    SchemaTable::new(
        "accounts".to_string(),
        vec![
            SchemaColumn::new(
                "tenant_id".to_string(),
                TypeFamily::I32,
                DataTypeInfo::from_opt_object(&DataType::default_for(TypeFamily::I32)),
            ),
            SchemaColumn::new(
                "user_id".to_string(),
                TypeFamily::I32,
                DataTypeInfo::from_opt_object(&DataType::default_for(TypeFamily::I32)),
            ),
            SchemaColumn::new(
                "name".to_string(),
                TypeFamily::String,
                DataTypeInfo::from_opt_object(&DataType::default_for(TypeFamily::String)),
            ),
        ],
        vec![0, 1],
        vec![2],
    )
}

#[test]
fn planner_uses_read_key_for_complete_primary_key_equality() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let meta_mgr = Arc::new(TestMetaMgr::new(composite_schema()));
        let x_contract = Arc::new(TestXContract::new());
        let planner = Planner::new(PlanCtx {
            tx_mgr: Arc::new(TestTxMgr),
            meta_mgr: meta_mgr.clone(),
            x_contract: x_contract.clone(),
            async_runtime: None,
        });

        let exec = planner
            .plan_query(BoundQuery::Select(BoundSelect {
                table_id: meta_mgr.table_id(),
                select_items: vec![BoundSelectItem::Column(BoundSelectColumn {
                    attr: 0,
                    output_name: "tenant_id".to_string(),
                })],
                tuple_desc: TupleFieldDesc::new(Vec::new()),
                predicate: BoundPredicate::KeyEq {
                    key: vec![(0, vec![1]), (1, vec![2])],
                },
                residual: Vec::new(),
            }))
            .await
            .unwrap();

        exec.open().await.unwrap();
        let _ = exec.next().await.unwrap();
        assert_eq!(x_contract.read_key_calls.load(Ordering::Relaxed), 1);
        assert_eq!(x_contract.read_range_calls.load(Ordering::Relaxed), 0);
    })
    .unwrap()
}

#[test]
fn planner_uses_read_range_for_primary_key_prefix_equality() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let meta_mgr = Arc::new(TestMetaMgr::new(composite_schema()));
        let x_contract = Arc::new(TestXContract::new());
        let planner = Planner::new(PlanCtx {
            tx_mgr: Arc::new(TestTxMgr),
            meta_mgr: meta_mgr.clone(),
            x_contract: x_contract.clone(),
            async_runtime: None,
        });

        let exec = planner
            .plan_query(BoundQuery::Select(BoundSelect {
                table_id: meta_mgr.table_id(),
                select_items: vec![BoundSelectItem::Column(BoundSelectColumn {
                    attr: 0,
                    output_name: "tenant_id".to_string(),
                })],
                tuple_desc: TupleFieldDesc::new(Vec::new()),
                predicate: BoundPredicate::KeyPrefixEq {
                    prefix: vec![(0, vec![1])],
                },
                residual: Vec::new(),
            }))
            .await
            .unwrap();

        exec.open().await.unwrap();
        assert_eq!(x_contract.read_key_calls.load(Ordering::Relaxed), 0);
        assert_eq!(x_contract.read_range_calls.load(Ordering::Relaxed), 1);
    })
    .unwrap()
}

#[test]
fn planner_wraps_aggregate_over_range_scan() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let meta_mgr = Arc::new(TestMetaMgr::new(composite_schema()));
        let x_contract = Arc::new(TestXContract::new());
        let planner = Planner::new(PlanCtx {
            tx_mgr: Arc::new(TestTxMgr),
            meta_mgr: meta_mgr.clone(),
            x_contract: x_contract.clone(),
            async_runtime: None,
        });

        let exec = planner
            .plan_query(BoundQuery::Select(BoundSelect {
                table_id: meta_mgr.table_id(),
                select_items: vec![BoundSelectItem::Aggregate(BoundAggregate {
                    func: AggregateFunc::Count,
                    arg: None,
                    result_type: DataType::default_for(TypeFamily::I64),
                    output_name: "count".to_string(),
                    nullable: false,
                })],
                tuple_desc: TupleFieldDesc::new(Vec::new()),
                predicate: BoundPredicate::True,
                residual: Vec::new(),
            }))
            .await
            .unwrap();

        exec.open().await.unwrap();
        // The aggregate emits exactly one row even over an empty scan.
        let row = exec.next().await.unwrap().unwrap();
        let count = TypedBin::new(TypeFamily::I64, row.fields()[0].clone().unwrap())
            .to_value(&DataType::default_for(TypeFamily::I64))
            .unwrap();
        assert_eq!(count.to_i64(), 0);
        assert!(exec.next().await.unwrap().is_none());
        assert_eq!(x_contract.read_range_calls.load(Ordering::Relaxed), 1);
    })
    .unwrap()
}

#[test]
fn planner_wraps_residual_filter_over_scan() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let meta_mgr = Arc::new(TestMetaMgr::new(composite_schema()));
        let x_contract = Arc::new(TestXContract::new());
        let planner = Planner::new(PlanCtx {
            tx_mgr: Arc::new(TestTxMgr),
            meta_mgr: meta_mgr.clone(),
            x_contract: x_contract.clone(),
            async_runtime: None,
        });

        let literal = mudu_type::data_value::DataValue::from_string("m".to_string())
            .to_binary(&DataType::default_for(TypeFamily::String))
            .unwrap()
            .into();
        let exec = planner
            .plan_query(BoundQuery::Select(BoundSelect {
                table_id: meta_mgr.table_id(),
                select_items: vec![BoundSelectItem::Column(BoundSelectColumn {
                    attr: 0,
                    output_name: "tenant_id".to_string(),
                })],
                tuple_desc: TupleFieldDesc::new(Vec::new()),
                predicate: BoundPredicate::KeyPrefixEq {
                    prefix: vec![(0, vec![1])],
                },
                residual: vec![BoundResidual {
                    attr: 2,
                    op: sql_parser::ast::expr_operator::ValueCompare::LT,
                    literal: Some(literal),
                }],
            }))
            .await
            .unwrap();

        exec.open().await.unwrap();
        // The empty scan produces no rows through the filter.
        assert!(exec.next().await.unwrap().is_none());
        assert_eq!(x_contract.read_range_calls.load(Ordering::Relaxed), 1);
    })
    .unwrap()
}

// Fixture self-tests: drive the mock trait surfaces so the shared fixtures
// are verified end to end.
#[test]
fn test_fixtures_cover_their_trait_surfaces() {
    let tx = TestTxMgr;
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

    // MetaMgr fixture: the not-found error path.
    let meta = TestMetaMgr::new(composite_schema());
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        meta.initialize().await.unwrap();
        match meta.get_table_by_id(999_999).await {
            Ok(_) => panic!("expected EntityNotFound for a missing table"),
            Err(err) => assert_eq!(err.ec(), mudu::error::ErrorCode::EntityNotFound),
        }
    })
    .unwrap();
}
