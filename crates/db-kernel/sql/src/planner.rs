use crate::bound_stmt::{
    BoundAck, BoundCommand, BoundCopyFrom, BoundCopyTo, BoundCreateConsumer, BoundCreateFsType,
    BoundCreatePartitionPlacement, BoundCreatePartitionRule, BoundCreateQueue, BoundCreateTable,
    BoundDelete, BoundDropConsumer, BoundDropQueue, BoundDropTable, BoundDropType, BoundInsert,
    BoundPredicate, BoundQuery, BoundReceive, BoundSelect, BoundSelectItem, BoundSetValue,
    BoundUpdate,
};
use crate::plan_ctx::PlanCtx;
use ::catalog::contract::cmd_exec::CmdExec;
use ::catalog::contract::query_exec::QueryExec;
use ::catalog::contract::queue::QueueWait;
use ::catalog::contract::table_desc::TableDesc;
use ::command::ack_queue::AckQueue;
use ::command::create_consumer::CreateConsumer;
use ::command::create_fs_type::CreateFsType;
use ::command::create_partition_placement::CreatePartitionPlacement;
use ::command::create_partition_rule::CreatePartitionRule;
use ::command::create_queue::CreateQueue;
use ::command::create_table::CreateTable;
use ::command::delete_key_value::DeleteKeyValue;
use ::command::drop_consumer::DropConsumer;
use ::command::drop_fs_type::DropFsType;
use ::command::drop_queue::DropQueue;
use ::command::drop_table::DropTable;
use ::command::insert_key_value::InsertKeyValue;
use ::command::load_from_file::{LoadFromFile, LoadFromFileParams};
use ::command::receive_queue::ReceiveQueue;
use ::command::save_to_file::{SaveToFile, SaveToFileParams};
use ::command::update_key_value::UpdateKeyValue;
use ::xengine::api::{DeltaAssign, OptRead, Predicate, RangeData, VecDatum, VecSelTerm};
use ::xengine::x_param::{
    PAccessKey, PAccessRange, PAckQueue, PCreateConsumer, PCreateFsType, PCreatePartitionPlacement,
    PCreatePartitionRule, PCreateQueue, PCreateTable, PDeleteKeyValue, PDropConsumer, PDropQueue,
    PDropTable, PDropType, PInsertKeyValue, PReceiveQueue, PUpdateKeyValue,
};
use mudu::common::id::AttrIndex;
use mudu::common::result::RS;
use mudu::error::ErrorCode as ER;
use mudu::mudu_error;
use sql_parser::ast::stmt_receive::ReceiveWait;
use std::sync::Arc;

/// Append `attr` to `attrs` unless already present.
fn push_unique(attrs: &mut Vec<AttrIndex>, attr: AttrIndex) {
    if !attrs.contains(&attr) {
        attrs.push(attr);
    }
}

/// Turns bound statements into executable query and command objects.
pub struct Planner {
    ctx: PlanCtx,
}

impl Planner {
    /// Creates a planner whose executors share the context `ctx`.
    pub fn new(ctx: PlanCtx) -> Self {
        Self { ctx }
    }

    /// Plans a bound query into a query executor.
    pub async fn plan_query(&self, query: BoundQuery) -> RS<Arc<dyn QueryExec>> {
        match query {
            BoundQuery::Select(select) => self.plan_select(select).await,
            BoundQuery::Receive(receive) => Ok(Arc::new(self.plan_receive(receive))),
        }
    }

    /// Plans a bound command into a command executor.
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
            BoundCommand::CreateQueue(stmt) => Ok(Arc::new(self.plan_create_queue(stmt))),
            BoundCommand::DropQueue(stmt) => Ok(Arc::new(self.plan_drop_queue(stmt))),
            BoundCommand::CreateConsumer(stmt) => Ok(Arc::new(self.plan_create_consumer(stmt))),
            BoundCommand::DropConsumer(stmt) => Ok(Arc::new(self.plan_drop_consumer(stmt))),
            BoundCommand::Ack(stmt) => Ok(Arc::new(self.plan_ack(stmt))),
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

        let scan_attrs = Self::scan_attrs_of(&stmt, &table_desc)?;
        // Resolve residual filters against the scan row layout before any
        // executor is built.
        let filters = Self::residual_filters(&stmt, &table_desc, &scan_attrs)?;
        let scan_desc =
            crate::executor::project_tuple_desc(&table_desc, &VecSelTerm::new(scan_attrs.clone()));
        let scan = self
            .plan_scan(&stmt, VecSelTerm::new(scan_attrs.clone()))
            .await?;

        if has_aggregate {
            Self::plan_aggregate_select(stmt, &table_desc, &scan_attrs, scan_desc, scan, filters)
        } else {
            Self::plan_projection_select(stmt, &table_desc, &scan_attrs, scan, filters)
        }
    }

    /// Columns the storage scan must produce, deduplicated in first-use
    /// order: output columns, aggregate arguments and residual filter
    /// columns.
    fn scan_attrs_of(stmt: &BoundSelect, table_desc: &TableDesc) -> RS<Vec<AttrIndex>> {
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
        Ok(scan_attrs)
    }

    /// Position of `attr` in the scan row layout.
    fn attr_position(scan_attrs: &[AttrIndex], attr: AttrIndex) -> RS<usize> {
        scan_attrs
            .iter()
            .position(|a| *a == attr)
            .ok_or_else(|| mudu_error!(ER::InvalidState, "attribute missing from scan list"))
    }

    /// Residual filters resolved against the scan row layout.
    fn residual_filters(
        stmt: &BoundSelect,
        table_desc: &TableDesc,
        scan_attrs: &[AttrIndex],
    ) -> RS<Vec<crate::executor::filter::ResidualFilter>> {
        let mut filters = Vec::with_capacity(stmt.residual.len());
        for residual in &stmt.residual {
            filters.push(crate::executor::filter::ResidualFilter {
                input_pos: Self::attr_position(scan_attrs, residual.attr)?,
                data_type: table_desc.get_attr(residual.attr).type_desc().clone(),
                op: residual.op,
                literal: residual.literal.clone(),
            });
        }
        Ok(filters)
    }

    /// Plans an aggregate select list: the filter only passes rows through;
    /// the aggregate executor performs the final projection.
    fn plan_aggregate_select(
        stmt: BoundSelect,
        table_desc: &TableDesc,
        scan_attrs: &[AttrIndex],
        scan_desc: mudu_contract::tuple::tuple_field_desc::TupleFieldDesc,
        scan: Arc<dyn QueryExec>,
        filters: Vec<crate::executor::filter::ResidualFilter>,
    ) -> RS<Arc<dyn QueryExec>> {
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
                        arg_pos: aggregate
                            .arg
                            .map(|attr| Self::attr_position(scan_attrs, attr))
                            .transpose()?,
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
        Ok(Arc::new(crate::executor::aggregate::AggregateExec::new(
            stmt.tuple_desc.clone(),
            child,
            specs,
        )))
    }

    /// Plans a plain column projection: use the scan directly when its row
    /// layout already matches the output exactly. Aggregates were handled by
    /// the caller, so every item here is a column.
    fn plan_projection_select(
        stmt: BoundSelect,
        table_desc: &TableDesc,
        scan_attrs: &[AttrIndex],
        scan: Arc<dyn QueryExec>,
        filters: Vec<crate::executor::filter::ResidualFilter>,
    ) -> RS<Arc<dyn QueryExec>> {
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
            .map(|attr| Self::attr_position(scan_attrs, *attr))
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
        let mut null_assignments = Vec::new();
        for (attr, set_value) in stmt.value {
            match set_value {
                BoundSetValue::Absolute(binary) => absolute.push((attr, binary)),
                BoundSetValue::Null => null_assignments.push(attr),
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
                null_assignments,
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

/// Queue statement planning methods (`planner/queue.rs`).
mod queue;

#[cfg(test)]
#[path = "planner_test.rs"]
mod tests;
