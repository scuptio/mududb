use crate::command::create_fs_type::CreateFsType;
use crate::command::create_partition_placement::CreatePartitionPlacement;
use crate::command::create_partition_rule::CreatePartitionRule;
use crate::command::create_table::CreateTable;
use crate::command::delete_key_value::DeleteKeyValue;
use crate::command::drop_fs_type::DropFsType;
use crate::command::drop_table::DropTable;
use crate::command::insert_key_value::InsertKeyValue;
use crate::command::load_from_file::{LoadFromFile, LoadFromFileParams};
use crate::command::save_to_file::{SaveToFile, SaveToFileParams};
use crate::command::update_key_value::UpdateKeyValue;
use crate::contract::cmd_exec::CmdExec;
use crate::contract::query_exec::QueryExec;
use crate::sql::bound_stmt::{
    BoundCommand, BoundCopyFrom, BoundCopyTo, BoundCreateFsType, BoundCreatePartitionPlacement,
    BoundCreatePartitionRule, BoundCreateTable, BoundDelete, BoundDropTable, BoundDropType,
    BoundInsert, BoundPredicate, BoundQuery, BoundSelect, BoundSelectItem, BoundSetValue,
    BoundUpdate,
};
use crate::sql::plan_ctx::PlanCtx;
use crate::x_engine::api::{DeltaAssign, OptRead, Predicate, RangeData, VecDatum, VecSelTerm};
use crate::x_engine::x_param::{
    PAccessKey, PAccessRange, PCreateFsType, PCreatePartitionPlacement, PCreatePartitionRule,
    PCreateTable, PDeleteKeyValue, PDropTable, PDropType, PInsertKeyValue, PUpdateKeyValue,
};
use mudu::common::id::AttrIndex;
use mudu::common::result::RS;
use mudu::error::ErrorCode as ER;
use mudu::mudu_error;
use std::sync::Arc;

/// Append `attr` to `attrs` unless already present.
fn push_unique(attrs: &mut Vec<AttrIndex>, attr: AttrIndex) {
    if !attrs.contains(&attr) {
        attrs.push(attr);
    }
}

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
            BoundCommand::CreateFsType(stmt) => Ok(Arc::new(self.plan_create_fs_type(stmt))),
            BoundCommand::DropType(stmt) => Ok(Arc::new(self.plan_drop_fs_type(stmt))),
            BoundCommand::Insert(stmt) => Ok(Arc::new(self.plan_insert(stmt))),
            BoundCommand::Update(stmt) => Ok(Arc::new(self.plan_update(stmt))),
            BoundCommand::Delete(stmt) => Ok(Arc::new(self.plan_delete(stmt))),
            BoundCommand::CopyFrom(stmt) => Ok(Arc::new(self.plan_copy_from(stmt))),
            BoundCommand::CopyTo(stmt) => Ok(Arc::new(self.plan_copy_to(stmt))),
        }
    }

    async fn plan_select(&self, stmt: BoundSelect) -> RS<Arc<dyn QueryExec>> {
        let table_desc = self.ctx.meta_mgr.get_table_by_id(stmt.table_id).await?;

        let has_aggregate = stmt
            .select_items
            .iter()
            .any(|item| matches!(item, BoundSelectItem::Aggregate(_)));

        // Columns the storage scan must produce, deduplicated in first-use
        // order: output columns, aggregate arguments and residual filter
        // columns.
        let mut scan_attrs: Vec<AttrIndex> = Vec::new();
        for item in &stmt.select_items {
            match item {
                BoundSelectItem::Column(column) => push_unique(&mut scan_attrs, column.attr),
                BoundSelectItem::Aggregate(aggregate) => {
                    if let Some(attr) = aggregate.arg {
                        push_unique(&mut scan_attrs, attr);
                    }
                }
            }
        }
        for residual in &stmt.residual {
            push_unique(&mut scan_attrs, residual.attr);
        }
        if scan_attrs.is_empty() {
            // Pure `COUNT(*)`: the scan still needs one column to drive the
            // row count; the first key column is the cheapest.
            let attr = *table_desc
                .key_indices()
                .first()
                .ok_or_else(|| mudu_error!(ER::InvalidState, "table has no key columns"))?;
            scan_attrs.push(attr);
        }
        let attr_pos = |attr: AttrIndex| -> RS<usize> {
            scan_attrs
                .iter()
                .position(|a| *a == attr)
                .ok_or_else(|| mudu_error!(ER::InvalidState, "attribute missing from scan list"))
        };

        // Resolve residual filters against the scan row layout before any
        // executor is built.
        let mut filters = Vec::with_capacity(stmt.residual.len());
        for residual in &stmt.residual {
            filters.push(crate::executor::filter::ResidualFilter {
                input_pos: attr_pos(residual.attr)?,
                data_type: table_desc.get_attr(residual.attr).type_desc().clone(),
                op: residual.op,
                literal: residual.literal.clone(),
            });
        }
        let scan_desc =
            crate::executor::project_tuple_desc(&table_desc, &VecSelTerm::new(scan_attrs.clone()));
        let scan = self
            .plan_scan(&stmt, VecSelTerm::new(scan_attrs.clone()))
            .await?;

        if has_aggregate {
            // With aggregates the filter only passes rows through; the
            // aggregate executor performs the final projection.
            let child = if filters.is_empty() {
                scan
            } else {
                Arc::new(crate::executor::filter::FilterExec::new(
                    scan_desc,
                    scan,
                    filters,
                    (0..scan_attrs.len()).collect(),
                ))
            };
            let specs = stmt
                .select_items
                .iter()
                .map(|item| match item {
                    BoundSelectItem::Aggregate(aggregate) => {
                        Ok(crate::executor::aggregate::AggregateSpec {
                            func: aggregate.func,
                            arg_pos: aggregate.arg.map(attr_pos).transpose()?,
                            arg_type: aggregate
                                .arg
                                .map(|attr| table_desc.get_attr(attr).type_desc().clone()),
                            result_type: aggregate.result_type.clone(),
                        })
                    }
                    BoundSelectItem::Column(_) => Err(mudu_error!(
                        ER::InvalidState,
                        "plain column in an aggregate select list"
                    )),
                })
                .collect::<RS<Vec<_>>>()?;
            return Ok(Arc::new(crate::executor::aggregate::AggregateExec::new(
                stmt.tuple_desc.clone(),
                child,
                specs,
            )));
        }

        // Plain column projection: use the scan directly when its row layout
        // already matches the output exactly. Aggregates were handled above,
        // so every item here is a column.
        let mut output_attrs: Vec<AttrIndex> = Vec::with_capacity(stmt.select_items.len());
        for item in &stmt.select_items {
            let BoundSelectItem::Column(column) = item else {
                continue;
            };
            output_attrs.push(column.attr);
        }
        let mut direct = filters.is_empty() && output_attrs == scan_attrs;
        if direct {
            for item in &stmt.select_items {
                let BoundSelectItem::Column(column) = item else {
                    continue;
                };
                if table_desc.get_attr(column.attr).name() != &column.output_name {
                    direct = false;
                    break;
                }
            }
        }
        if direct {
            return Ok(scan);
        }
        let projection = output_attrs
            .iter()
            .map(|attr| attr_pos(*attr))
            .collect::<RS<Vec<_>>>()?;
        Ok(Arc::new(crate::executor::filter::FilterExec::new(
            stmt.tuple_desc.clone(),
            scan,
            filters,
            projection,
        )))
    }

    async fn plan_scan(&self, stmt: &BoundSelect, select: VecSelTerm) -> RS<Arc<dyn QueryExec>> {
        match &stmt.predicate {
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
                        pred_key: VecDatum::new(key.clone()),
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
                        pred_non_key: Predicate::KeyPrefixEq(prefix.clone()),
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
                        pred_key: RangeData::new(start.clone(), end.clone()),
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

    fn plan_create_fs_type(&self, stmt: BoundCreateFsType) -> CreateFsType {
        CreateFsType::new(
            PCreateFsType {
                name: stmt.name,
                kind: stmt.kind,
            },
            self.ctx.meta_mgr.clone(),
        )
    }

    fn plan_drop_fs_type(&self, stmt: BoundDropType) -> DropFsType {
        DropFsType::new(PDropType { name: stmt.name }, self.ctx.meta_mgr.clone())
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
        let mut absolute = Vec::new();
        let mut delta_assignments = Vec::new();
        for (attr, set_value) in stmt.value {
            match set_value {
                BoundSetValue::Absolute(binary) => absolute.push((attr, binary)),
                BoundSetValue::Delta { op, literal } => {
                    delta_assignments.push(DeltaAssign { attr, op, literal })
                }
            }
        }
        UpdateKeyValue::new(
            PUpdateKeyValue {
                tx_mgr: self.ctx.tx_mgr.clone(),
                table_id: stmt.table_id,
                key: VecDatum::new(stmt.key),
                value: VecDatum::new(absolute),
                delta_assignments,
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
        LoadFromFile::new(LoadFromFileParams {
            csv_file: stmt.file_path,
            tx_mgr: self.ctx.tx_mgr.clone(),
            table_id: stmt.table_id,
            key_index: stmt.key_index,
            value_index: stmt.value_index,
            x_contract: self.ctx.x_contract.clone(),
            meta_mgr: self.ctx.meta_mgr.clone(),
            async_runtime: self.ctx.async_runtime.clone(),
        })
    }

    fn plan_copy_to(&self, stmt: BoundCopyTo) -> SaveToFile {
        SaveToFile::new(SaveToFileParams {
            file_path: stmt.file_path,
            tx_mgr: self.ctx.tx_mgr.clone(),
            table_id: stmt.table_id,
            key_indexing: stmt.key_indexing,
            value_indexing: stmt.value_indexing,
            x_contract: self.ctx.x_contract.clone(),
            meta_mgr: self.ctx.meta_mgr.clone(),
            async_runtime: self.ctx.async_runtime.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::todo,
        clippy::unimplemented
    )]

    use super::Planner;
    use crate::contract::meta_mgr::MetaMgr;
    use crate::contract::schema_column::SchemaColumn;
    use crate::contract::schema_table::SchemaTable;
    use crate::contract::table_desc::TableDesc;
    use crate::contract::table_info::TableInfo;
    use crate::server::worker_snapshot::WorkerSnapshot;
    use crate::sql::bound_stmt::{
        AggregateFunc, BoundAggregate, BoundPredicate, BoundQuery, BoundResidual, BoundSelect,
        BoundSelectColumn, BoundSelectItem,
    };
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

        async fn get_table_by_name(&self, name: &str) -> RS<Option<Arc<TableDesc>>> {
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
}
