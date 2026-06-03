use crate::command::create_partition_placement::CreatePartitionPlacement;
use crate::command::create_partition_rule::CreatePartitionRule;
use crate::command::create_table::CreateTable;
use crate::command::delete_key_value::DeleteKeyValue;
use crate::command::drop_table::DropTable;
use crate::command::insert_key_value::InsertKeyValue;
use crate::command::load_from_file::LoadFromFile;
use crate::command::save_to_file::SaveToFile;
use crate::command::update_key_value::UpdateKeyValue;
use crate::contract::cmd_exec::CmdExec;
use crate::contract::query_exec::QueryExec;
use crate::sql::bound_stmt::{
    BoundCommand, BoundCopyFrom, BoundCopyTo, BoundCreatePartitionPlacement,
    BoundCreatePartitionRule, BoundCreateTable, BoundDelete, BoundDropTable, BoundInsert,
    BoundPredicate, BoundQuery, BoundSelect, BoundUpdate,
};
use crate::sql::plan_ctx::PlanCtx;
use crate::x_engine::api::{OptRead, Predicate, RangeData, VecDatum, VecSelTerm};
use crate::x_engine::x_param::{
    PAccessKey, PAccessRange, PCreatePartitionPlacement, PCreatePartitionRule, PCreateTable,
    PDeleteKeyValue, PDropTable, PInsertKeyValue, PUpdateKeyValue,
};
use mudu::common::result::RS;
use std::sync::Arc;

pub struct Planner {
    ctx: PlanCtx,
}

impl Planner {
    pub fn new(ctx: PlanCtx) -> Self {
        Self { ctx }
    }

    pub async fn plan_query(&self, query: BoundQuery) -> RS<Arc<dyn QueryExec>> {
        match query {
            BoundQuery::Select(select) => self.plan_select(select).await,
        }
    }

    pub async fn plan_command(&self, command: BoundCommand) -> RS<Arc<dyn CmdExec>> {
        match command {
            BoundCommand::CreatePartitionPlacement(stmt) => {
                Ok(Arc::new(self.plan_create_partition_placement(stmt)))
            }
            BoundCommand::CreatePartitionRule(stmt) => {
                Ok(Arc::new(self.plan_create_partition_rule(stmt)))
            }
            BoundCommand::CreateTable(stmt) => Ok(Arc::new(self.plan_create_table(stmt))),
            BoundCommand::DropTable(stmt) => Ok(Arc::new(self.plan_drop_table(stmt))),
            BoundCommand::Insert(stmt) => Ok(Arc::new(self.plan_insert(stmt))),
            BoundCommand::Update(stmt) => Ok(Arc::new(self.plan_update(stmt))),
            BoundCommand::Delete(stmt) => Ok(Arc::new(self.plan_delete(stmt))),
            BoundCommand::CopyFrom(stmt) => Ok(Arc::new(self.plan_copy_from(stmt))),
            BoundCommand::CopyTo(stmt) => Ok(Arc::new(self.plan_copy_to(stmt))),
        }
    }

    async fn plan_select(&self, stmt: BoundSelect) -> RS<Arc<dyn QueryExec>> {
        let select = VecSelTerm::new(stmt.select_attrs.clone());
        match stmt.predicate {
            BoundPredicate::True => {
                let exec = crate::executor::index_access_range::IndexAccessRange::new(
                    PAccessRange {
                        tx_mgr: self.ctx.tx_mgr.clone(),
                        table_id: stmt.table_id,
                        pred_key: RangeData::new(
                            std::ops::Bound::Unbounded,
                            std::ops::Bound::Unbounded,
                        ),
                        pred_non_key: Predicate::CNF(Vec::new()),
                        select,
                        opt_read: OptRead::default(),
                    },
                    self.ctx.x_contract.clone(),
                    self.ctx.meta_mgr.clone(),
                )
                .await?;
                Ok(Arc::new(exec))
            }
            BoundPredicate::KeyEq { key } => {
                let exec = crate::executor::index_access_key::IndexAccessKey::new(
                    PAccessKey {
                        tx_mgr: self.ctx.tx_mgr.clone(),
                        table_id: stmt.table_id,
                        pred_key: VecDatum::new(key),
                        select,
                        opt_read: OptRead::default(),
                    },
                    self.ctx.x_contract.clone(),
                    self.ctx.meta_mgr.clone(),
                )
                .await?;
                Ok(Arc::new(exec))
            }
            BoundPredicate::KeyPrefixEq { prefix } => {
                let exec = crate::executor::index_access_range::IndexAccessRange::new(
                    PAccessRange {
                        tx_mgr: self.ctx.tx_mgr.clone(),
                        table_id: stmt.table_id,
                        pred_key: RangeData::new(
                            std::ops::Bound::Unbounded,
                            std::ops::Bound::Unbounded,
                        ),
                        pred_non_key: Predicate::KeyPrefixEq(prefix),
                        select,
                        opt_read: OptRead::default(),
                    },
                    self.ctx.x_contract.clone(),
                    self.ctx.meta_mgr.clone(),
                )
                .await?;
                Ok(Arc::new(exec))
            }
            BoundPredicate::KeyRange { start, end } => {
                let exec = crate::executor::index_access_range::IndexAccessRange::new(
                    PAccessRange {
                        tx_mgr: self.ctx.tx_mgr.clone(),
                        table_id: stmt.table_id,
                        pred_key: RangeData::new(start, end),
                        pred_non_key: Predicate::CNF(Vec::new()),
                        select,
                        opt_read: OptRead::default(),
                    },
                    self.ctx.x_contract.clone(),
                    self.ctx.meta_mgr.clone(),
                )
                .await?;
                Ok(Arc::new(exec))
            }
        }
    }

    fn plan_create_partition_placement(
        &self,
        stmt: BoundCreatePartitionPlacement,
    ) -> CreatePartitionPlacement {
        CreatePartitionPlacement::new(
            PCreatePartitionPlacement {
                tx_mgr: self.ctx.tx_mgr.clone(),
                placements: stmt.placements,
            },
            self.ctx.meta_mgr.clone(),
        )
    }

    fn plan_create_partition_rule(&self, stmt: BoundCreatePartitionRule) -> CreatePartitionRule {
        CreatePartitionRule::new(
            PCreatePartitionRule {
                tx_mgr: self.ctx.tx_mgr.clone(),
                rule: stmt.rule,
            },
            self.ctx.meta_mgr.clone(),
        )
    }

    fn plan_create_table(&self, stmt: BoundCreateTable) -> CreateTable {
        CreateTable::new(
            PCreateTable {
                tx_mgr: self.ctx.tx_mgr.clone(),
                schema: stmt.schema,
                partition_binding: stmt.partition_binding,
            },
            self.ctx.x_contract.clone(),
            self.ctx.meta_mgr.clone(),
        )
    }

    fn plan_drop_table(&self, stmt: BoundDropTable) -> DropTable {
        DropTable::new(
            PDropTable {
                tx_mgr: self.ctx.tx_mgr.clone(),
                oid: stmt.oid,
            },
            self.ctx.x_contract.clone(),
            self.ctx.meta_mgr.clone(),
        )
    }

    fn plan_insert(&self, stmt: BoundInsert) -> InsertKeyValue {
        InsertKeyValue::new(
            PInsertKeyValue {
                tx_mgr: self.ctx.tx_mgr.clone(),
                table_id: stmt.table_id,
                rows: stmt
                    .rows
                    .into_iter()
                    .map(|row| (VecDatum::new(row.key), VecDatum::new(row.value)))
                    .collect(),
            },
            self.ctx.x_contract.clone(),
            self.ctx.meta_mgr.clone(),
        )
    }

    fn plan_update(&self, stmt: BoundUpdate) -> UpdateKeyValue {
        UpdateKeyValue::new(
            PUpdateKeyValue {
                tx_mgr: self.ctx.tx_mgr.clone(),
                table_id: stmt.table_id,
                key: VecDatum::new(stmt.key),
                value: VecDatum::new(stmt.value),
            },
            self.ctx.x_contract.clone(),
            self.ctx.meta_mgr.clone(),
        )
    }

    fn plan_delete(&self, stmt: BoundDelete) -> DeleteKeyValue {
        DeleteKeyValue::new(
            PDeleteKeyValue {
                tx_mgr: self.ctx.tx_mgr.clone(),
                table_id: stmt.table_id,
                key: VecDatum::new(stmt.key),
            },
            self.ctx.x_contract.clone(),
            self.ctx.meta_mgr.clone(),
        )
    }

    fn plan_copy_from(&self, stmt: BoundCopyFrom) -> LoadFromFile {
        LoadFromFile::new(
            stmt.file_path,
            self.ctx.tx_mgr.clone(),
            stmt.table_id,
            stmt.key_index,
            stmt.value_index,
            self.ctx.x_contract.clone(),
            self.ctx.meta_mgr.clone(),
        )
    }

    fn plan_copy_to(&self, stmt: BoundCopyTo) -> SaveToFile {
        SaveToFile::new(
            stmt.file_path,
            self.ctx.tx_mgr.clone(),
            stmt.table_id,
            stmt.key_indexing,
            stmt.value_indexing,
            self.ctx.x_contract.clone(),
            self.ctx.meta_mgr.clone(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::Planner;
    use crate::contract::meta_mgr::MetaMgr;
    use crate::contract::schema_column::SchemaColumn;
    use crate::contract::schema_table::SchemaTable;
    use crate::contract::table_desc::TableDesc;
    use crate::contract::table_info::TableInfo;
    use crate::server::worker_snapshot::WorkerSnapshot;
    use crate::sql::bound_stmt::{BoundPredicate, BoundQuery, BoundSelect};
    use crate::sql::plan_ctx::PlanCtx;
    use crate::x_engine::api::{
        AlterTable, OptDelete, OptInsert, OptRead, OptUpdate, Predicate, RSCursor, RangeData,
        TupleRow, VecDatum, VecSelTerm, XContract,
    };
    use crate::x_engine::tx_mgr::{PhysicalRelationId, TxMgr};
    use async_trait::async_trait;
    use mudu::common::id::OID;
    use mudu::common::result::RS;
    use mudu_contract::tuple::tuple_field_desc::TupleFieldDesc;
    use mudu_type::dat_type::DatType;
    use mudu_type::dat_type_id::DatTypeID;
    use mudu_type::dt_info::DTInfo;
    use std::collections::{BTreeMap, HashMap};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
use mudu_sys::sync::SMutex;

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
        async fn get_table_by_id(&self, oid: OID) -> RS<Arc<TableDesc>> {
            self.tables
                .lock()
                .unwrap()
                .get(&oid)
                .cloned()
                .ok_or_else(|| mudu::m_error!(mudu::error::ec::EC::NoSuchElement, oid.to_string()))
        }

        async fn get_table_by_name(&self, name: &String) -> RS<Option<Arc<TableDesc>>> {
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
        fn xl_batch(&self) -> crate::wal::xl_batch::XLBatch {
            crate::wal::xl_batch::XLBatch::new(Vec::new())
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
                    DatTypeID::I32,
                    DTInfo::from_opt_object(&DatType::default_for(DatTypeID::I32)),
                ),
                SchemaColumn::new(
                    "user_id".to_string(),
                    DatTypeID::I32,
                    DTInfo::from_opt_object(&DatType::default_for(DatTypeID::I32)),
                ),
                SchemaColumn::new(
                    "name".to_string(),
                    DatTypeID::String,
                    DTInfo::from_opt_object(&DatType::default_for(DatTypeID::String)),
                ),
            ],
            vec![0, 1],
            vec![2],
        )
    }

    #[tokio::test]
    async fn planner_uses_read_key_for_complete_primary_key_equality() {
        let meta_mgr = Arc::new(TestMetaMgr::new(composite_schema()));
        let x_contract = Arc::new(TestXContract::new());
        let planner = Planner::new(PlanCtx {
            tx_mgr: Arc::new(TestTxMgr),
            meta_mgr: meta_mgr.clone(),
            x_contract: x_contract.clone(),
        });

        let exec = planner
            .plan_query(BoundQuery::Select(BoundSelect {
                table_id: meta_mgr.table_id(),
                select_attrs: vec![0],
                tuple_desc: TupleFieldDesc::new(Vec::new()),
                predicate: BoundPredicate::KeyEq {
                    key: vec![(0, vec![1]), (1, vec![2])],
                },
            }))
            .await
            .unwrap();

        exec.open().await.unwrap();
        let _ = exec.next().await.unwrap();
        assert_eq!(x_contract.read_key_calls.load(Ordering::Relaxed), 1);
        assert_eq!(x_contract.read_range_calls.load(Ordering::Relaxed), 0);
    }

    #[tokio::test]
    async fn planner_uses_read_range_for_primary_key_prefix_equality() {
        let meta_mgr = Arc::new(TestMetaMgr::new(composite_schema()));
        let x_contract = Arc::new(TestXContract::new());
        let planner = Planner::new(PlanCtx {
            tx_mgr: Arc::new(TestTxMgr),
            meta_mgr: meta_mgr.clone(),
            x_contract: x_contract.clone(),
        });

        let exec = planner
            .plan_query(BoundQuery::Select(BoundSelect {
                table_id: meta_mgr.table_id(),
                select_attrs: vec![0],
                tuple_desc: TupleFieldDesc::new(Vec::new()),
                predicate: BoundPredicate::KeyPrefixEq {
                    prefix: vec![(0, vec![1])],
                },
            }))
            .await
            .unwrap();

        exec.open().await.unwrap();
        assert_eq!(x_contract.read_key_calls.load(Ordering::Relaxed), 0);
        assert_eq!(x_contract.read_range_calls.load(Ordering::Relaxed), 1);
    }
}
