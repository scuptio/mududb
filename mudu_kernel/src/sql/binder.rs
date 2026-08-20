use crate::command::fs_hook::has_fs_bound_columns;
use crate::contract::fs_type::{FsColumnBinding, FsTypeKind};
use crate::contract::meta_mgr::MetaMgr;
use crate::contract::partition_rule::{PartitionBound, PartitionRuleDesc, RangePartitionDef};
use crate::contract::partition_rule_binding::{PartitionPlacement, TablePartitionBinding};
use crate::contract::schema_column::SchemaColumn;
use crate::contract::schema_table::SchemaTable;
use crate::contract::table_desc::TableDesc;
use crate::sql::bound_stmt::{
    BoundCommand, BoundCopyFrom, BoundCopyTo, BoundCreateFsType, BoundCreatePartitionPlacement,
    BoundCreatePartitionRule, BoundCreateTable, BoundDropTable, BoundDropType, BoundStmt,
};
use crate::sql::bound_template::{
    template_from_expr, BoundTemplate, DeleteTemplate, InsertRowTemplate, InsertTemplate,
    ParamSlot, PredicateTemplate, ResidualTemplate, SelectTemplate, SetValueTemplate, SlotRecorder,
    StmtTemplate, TemplateDatum, UpdateTemplate,
};
use crate::sql::copy_layout::CopyLayout;
use crate::sql::value_codec::ValueCodec;
use crate::x_engine::api::DeltaOp;
use mudu::common::result::RS;
use mudu::error::ErrorCode as ER;
use mudu::mudu_error;
use mudu_contract::database::sql_params::SQLParams;
use mudu_type::data_type::DataType;
use mudu_type::data_type_info::DataTypeInfo;
use mudu_type::type_family::TypeFamily;
use sql_parser::ast::expr_compare::ExprCompare;
use sql_parser::ast::expr_item::{ExprItem, ExprValue};
use sql_parser::ast::expr_literal::ExprLiteral;
use sql_parser::ast::expr_operator::{Arithmetic, ValueCompare};
use sql_parser::ast::expression::ExprType;
use sql_parser::ast::stmt_create_fs_type::{FsTypeKind as AstFsTypeKind, StmtCreateFsType};
use sql_parser::ast::stmt_create_partition_placement::StmtCreatePartitionPlacement;
use sql_parser::ast::stmt_create_partition_rule::{StmtCreatePartitionRule, StmtPartitionBound};
use sql_parser::ast::stmt_create_table::StmtCreateTable;
use sql_parser::ast::stmt_delete::StmtDelete;
use sql_parser::ast::stmt_drop_table::StmtDropTable;
use sql_parser::ast::stmt_drop_type::StmtDropType;
use sql_parser::ast::stmt_insert::StmtInsert;
use sql_parser::ast::stmt_type::{StmtCommand, StmtType};
use sql_parser::ast::stmt_update::{AssignedValue, StmtUpdate};
use std::ops::Bound;
use std::sync::Arc;

pub struct Binder {
    meta_mgr: Arc<dyn MetaMgr>,
}

impl Binder {
    pub fn new(meta_mgr: Arc<dyn MetaMgr>) -> Self {
        Self { meta_mgr }
    }

    pub async fn bind(&self, stmt: StmtType, params: &dyn SQLParams) -> RS<BoundStmt> {
        self.bind_ref(&stmt, params).await
    }

    /// Binds a borrowed statement. The hot DML arms (SELECT/INSERT/UPDATE/
    /// DELETE) only read the AST, so a statement shared from the parse cache
    /// is bound without a deep copy; the cold DDL arms clone the AST node
    /// they need to consume.
    ///
    /// DML statements are bound in template mode and immediately filled with
    /// `params` (see [`Self::bind_template`]); the result is identical to
    /// direct value binding.
    pub async fn bind_ref(&self, stmt: &StmtType, params: &dyn SQLParams) -> RS<BoundStmt> {
        match stmt {
            StmtType::Select(stmt) => {
                let mut recorder = SlotRecorder::default();
                let template = self.bind_select_template(stmt, &mut recorder).await?;
                BoundTemplate::new(StmtTemplate::Select(template), recorder.into_slots())
                    .fill(params)
            }
            StmtType::Command(command) => Ok(BoundStmt::Command(
                self.bind_command_ref(command, params).await?,
            )),
        }
    }

    /// Binds a DML statement into a parameter template without reading any
    /// parameter values; returns `None` for statements that are never
    /// templated (DDL, COPY).
    ///
    /// The template records each placeholder as an ordered [`ParamSlot`] so
    /// the result can be cached (keyed by SQL text and catalog version) and
    /// re-executed with different parameters via [`BoundTemplate::fill`].
    pub async fn bind_template(&self, stmt: &StmtType) -> RS<Option<BoundTemplate>> {
        let mut recorder = SlotRecorder::default();
        let stmt = match stmt {
            StmtType::Select(stmt) => {
                StmtTemplate::Select(self.bind_select_template(stmt, &mut recorder).await?)
            }
            StmtType::Command(StmtCommand::Insert(stmt)) => {
                StmtTemplate::Insert(self.bind_insert_template(stmt, &mut recorder).await?)
            }
            StmtType::Command(StmtCommand::Update(stmt)) => {
                StmtTemplate::Update(self.bind_update_template(stmt, &mut recorder).await?)
            }
            StmtType::Command(StmtCommand::Delete(stmt)) => {
                StmtTemplate::Delete(self.bind_delete_template(stmt, &mut recorder).await?)
            }
            _ => return Ok(None),
        };
        Ok(Some(BoundTemplate::new(stmt, recorder.into_slots())))
    }

    async fn bind_command_ref(
        &self,
        command: &StmtCommand,
        params: &dyn SQLParams,
    ) -> RS<BoundCommand> {
        match command {
            StmtCommand::Insert(stmt) => self.bind_insert_filled(stmt, params).await,
            StmtCommand::Update(stmt) => self.bind_update_filled(stmt, params).await,
            StmtCommand::Delete(stmt) => self.bind_delete_filled(stmt, params).await,
            other => self.bind_command(other.clone(), params).await,
        }
    }

    /// Template-binds `stmt` and immediately fills it with `params`; the
    /// result is identical to direct value binding.
    async fn bind_insert_filled(
        &self,
        stmt: &StmtInsert,
        params: &dyn SQLParams,
    ) -> RS<BoundCommand> {
        let mut recorder = SlotRecorder::default();
        let template = self.bind_insert_template(stmt, &mut recorder).await?;
        Self::fill_command(
            StmtTemplate::Insert(template),
            recorder.into_slots(),
            params,
        )
    }

    async fn bind_update_filled(
        &self,
        stmt: &StmtUpdate,
        params: &dyn SQLParams,
    ) -> RS<BoundCommand> {
        let mut recorder = SlotRecorder::default();
        let template = self.bind_update_template(stmt, &mut recorder).await?;
        Self::fill_command(
            StmtTemplate::Update(template),
            recorder.into_slots(),
            params,
        )
    }

    async fn bind_delete_filled(
        &self,
        stmt: &StmtDelete,
        params: &dyn SQLParams,
    ) -> RS<BoundCommand> {
        let mut recorder = SlotRecorder::default();
        let template = self.bind_delete_template(stmt, &mut recorder).await?;
        Self::fill_command(
            StmtTemplate::Delete(template),
            recorder.into_slots(),
            params,
        )
    }

    fn fill_command(
        template: StmtTemplate,
        slots: Vec<ParamSlot>,
        params: &dyn SQLParams,
    ) -> RS<BoundCommand> {
        match BoundTemplate::new(template, slots).fill(params)? {
            BoundStmt::Command(command) => Ok(command),
            BoundStmt::Query(_) => Err(mudu_error!(
                ER::Internal,
                "query template filled as command"
            )),
        }
    }

    async fn bind_command(&self, command: StmtCommand, params: &dyn SQLParams) -> RS<BoundCommand> {
        match command {
            StmtCommand::CreatePartitionPlacement(stmt) => {
                Ok(BoundCommand::CreatePartitionPlacement(
                    self.bind_create_partition_placement(stmt).await?,
                ))
            }
            StmtCommand::CreatePartitionRule(stmt) => Ok(BoundCommand::CreatePartitionRule(
                self.bind_create_partition_rule(stmt)?,
            )),
            StmtCommand::CreateTable(stmt) => Ok(BoundCommand::CreateTable(
                self.bind_create_table(stmt).await?,
            )),
            StmtCommand::DropTable(stmt) => {
                Ok(BoundCommand::DropTable(self.bind_drop_table(stmt).await?))
            }
            StmtCommand::CreateFsType(stmt) => {
                Ok(BoundCommand::CreateFsType(Self::bind_create_fs_type(&stmt)))
            }
            StmtCommand::DropType(stmt) => Ok(BoundCommand::DropType(Self::bind_drop_type(&stmt))),
            StmtCommand::Insert(stmt) => self.bind_insert_filled(&stmt, params).await,
            StmtCommand::Update(stmt) => self.bind_update_filled(&stmt, params).await,
            StmtCommand::Delete(stmt) => self.bind_delete_filled(&stmt, params).await,
            StmtCommand::CopyFrom(stmt) => {
                Ok(BoundCommand::CopyFrom(self.bind_copy_from(stmt).await?))
            }
            StmtCommand::CopyTo(stmt) => Ok(BoundCommand::CopyTo(self.bind_copy_to(stmt).await?)),
        }
    }

    async fn bind_select_template(
        &self,
        stmt: &sql_parser::ast::stmt_select::StmtSelect,
        recorder: &mut SlotRecorder,
    ) -> RS<SelectTemplate> {
        let table_desc = self.get_table_by_name(stmt.get_table_reference()).await?;
        let (select_items, tuple_desc) = crate::sql::select_projection::bind_select_items(
            &table_desc,
            stmt.get_select_term_list(),
        )?;
        let (predicate, residual) =
            self.bind_predicate_template(&table_desc, stmt.get_where_predicate(), recorder)?;
        Ok(SelectTemplate {
            table_id: table_desc.id(),
            select_items,
            tuple_desc,
            predicate,
            residual,
            has_fs_columns: has_fs_bound_columns(&table_desc),
        })
    }

    async fn bind_create_table(&self, mut stmt: StmtCreateTable) -> RS<BoundCreateTable> {
        stmt.assign_index_for_columns();
        let mut columns = Vec::new();
        for column in stmt.primary_columns() {
            columns.push(self.schema_column_from_ast(column).await?);
        }
        let value_offset = columns.len();
        let mut value_columns = Vec::new();
        for column in stmt.non_primary_columns() {
            value_columns.push(self.schema_column_from_ast(column).await?);
        }
        let key_indices = (0..columns.len()).collect();
        let value_indices = (0..value_columns.len())
            .map(|index| index + value_offset)
            .collect();
        columns.append(&mut value_columns);
        let schema = SchemaTable::new(
            stmt.table_name().clone(),
            columns,
            key_indices,
            value_indices,
        );
        let partition_binding = if let Some(partition) = stmt.partition() {
            let rule = self
                .meta_mgr
                .get_partition_rule_by_name(partition.rule_name())
                .await?
                .ok_or_else(|| {
                    mudu_error!(
                        ER::EntityNotFound,
                        format!("no such partition rule {}", partition.rule_name())
                    )
                })?;
            let ref_attr_indices = partition
                .reference_columns()
                .iter()
                .map(|column| {
                    schema
                        .columns()
                        .iter()
                        .position(|field| field.get_name() == column)
                        .ok_or_else(|| {
                            mudu_error!(
                                ER::EntityNotFound,
                                format!("no such partition reference column {}", column)
                            )
                        })
                })
                .collect::<RS<Vec<_>>>()?;
            if rule.partitions.is_empty() {
                return Err(mudu_error!(
                    ER::Parse,
                    format!("partition rule {} has no partitions", partition.rule_name())
                ));
            }
            Some(TablePartitionBinding {
                table_id: schema.id(),
                rule_id: rule.oid,
                ref_attr_indices,
            })
        } else {
            None
        };
        Ok(BoundCreateTable {
            schema,
            partition_binding,
        })
    }

    fn bind_create_partition_rule(
        &self,
        stmt: StmtCreatePartitionRule,
    ) -> RS<BoundCreatePartitionRule> {
        let partitions = stmt
            .partitions()
            .iter()
            .map(|partition| {
                Ok(RangePartitionDef::new(
                    partition.name().to_string(),
                    Self::bind_partition_bound(partition.start()),
                    Self::bind_partition_bound(partition.end()),
                ))
            })
            .collect::<RS<Vec<_>>>()?;
        Ok(BoundCreatePartitionRule {
            rule: PartitionRuleDesc::new_range(
                stmt.rule_name().to_string(),
                Self::infer_partition_rule_key_types(stmt.partitions())?,
                partitions,
            ),
        })
    }

    fn infer_partition_rule_key_types(
        partitions: &[sql_parser::ast::stmt_create_partition_rule::StmtRangePartition],
    ) -> RS<Vec<TypeFamily>> {
        let mut width = None;
        let mut type_slots: Vec<InferredKeyType> = Vec::new();

        for partition in partitions {
            for bound in [partition.start(), partition.end()] {
                let values = match bound {
                    StmtPartitionBound::Unbounded => continue,
                    StmtPartitionBound::Value(values) => values,
                };
                if let Some(expected) = width {
                    if expected != values.len() {
                        return Err(mudu_error!(
                            ER::Parse,
                            "partition bound width mismatch in CREATE PARTITION RULE"
                        ));
                    }
                } else {
                    width = Some(values.len());
                    type_slots = vec![InferredKeyType::I64; values.len()];
                }

                for (index, raw) in values.iter().enumerate() {
                    let next = infer_textual_value_type(raw)?;
                    type_slots[index] = type_slots[index].merge(next);
                }
            }
        }

        match width {
            Some(_) => Ok(type_slots
                .into_iter()
                .map(|item| item.to_type_family())
                .collect()),
            None => Err(mudu_error!(
                ER::Parse,
                "cannot infer partition key types from unbounded rule"
            )),
        }
    }

    async fn bind_create_partition_placement(
        &self,
        stmt: StmtCreatePartitionPlacement,
    ) -> RS<BoundCreatePartitionPlacement> {
        let rule = self
            .meta_mgr
            .get_partition_rule_by_name(stmt.rule_name())
            .await?
            .ok_or_else(|| {
                mudu_error!(
                    ER::EntityNotFound,
                    format!("no such partition rule {}", stmt.rule_name())
                )
            })?;
        let mut placements = Vec::with_capacity(stmt.placements().len());
        for placement in stmt.placements() {
            let partition = rule
                .partitions
                .iter()
                .find(|partition| partition.name == placement.partition_name())
                .ok_or_else(|| {
                    mudu_error!(
                        ER::EntityNotFound,
                        format!(
                            "no such partition {} in rule {}",
                            placement.partition_name(),
                            stmt.rule_name()
                        )
                    )
                })?;
            let worker_id = placement.worker_id().parse::<u128>().map_err(|e| {
                mudu_error!(
                    ER::Parse,
                    format!("invalid worker id {}", placement.worker_id()),
                    e
                )
            })?;
            placements.push(PartitionPlacement {
                partition_id: partition.partition_id,
                worker_id,
            });
        }
        Ok(BoundCreatePartitionPlacement { placements })
    }

    fn bind_partition_bound(bound: &StmtPartitionBound) -> PartitionBound {
        match bound {
            StmtPartitionBound::Unbounded => PartitionBound::Unbounded,
            StmtPartitionBound::Value(values) => PartitionBound::Value(values.clone()),
        }
    }

    async fn bind_drop_table(&self, stmt: StmtDropTable) -> RS<BoundDropTable> {
        match self.meta_mgr.get_table_by_name(stmt.table_name()).await? {
            Some(table_desc) => Ok(BoundDropTable {
                oid: Some(table_desc.id()),
            }),
            None if stmt.drop_if_exists() => Ok(BoundDropTable { oid: None }),
            None => Err(mudu_error!(
                ER::EntityNotFound,
                format!("cannot find table {}", stmt.table_name())
            )),
        }
    }

    fn bind_create_fs_type(stmt: &StmtCreateFsType) -> BoundCreateFsType {
        let kind = match stmt.kind() {
            AstFsTypeKind::File => FsTypeKind::File,
            AstFsTypeKind::Directory => FsTypeKind::Directory,
        };
        BoundCreateFsType {
            name: stmt.name().to_string(),
            kind,
        }
    }

    fn bind_drop_type(stmt: &StmtDropType) -> BoundDropType {
        BoundDropType {
            name: stmt.name().to_string(),
        }
    }

    async fn bind_insert_template(
        &self,
        stmt: &StmtInsert,
        recorder: &mut SlotRecorder,
    ) -> RS<InsertTemplate> {
        let table_desc = self.get_table_by_name(stmt.table_name()).await?;

        let columns = if stmt.columns().is_empty() {
            let total = table_desc.fields().len();
            (0..total)
                .map(|attr| table_desc.get_attr(attr).name().clone())
                .collect::<Vec<_>>()
        } else {
            stmt.columns().clone()
        };

        let mut rows = Vec::with_capacity(stmt.values_list().len());
        for values in stmt.values_list() {
            if columns.len() != values.len() {
                return Err(mudu_error!(
                    ER::InvalidArgument,
                    "insert column size mismatch"
                ));
            }

            let mut key = vec![];
            let mut value = vec![];
            for (name, expr) in columns.iter().zip(values.iter()) {
                let attr = self.attr_index_by_name(&table_desc, name)?;
                let field = table_desc.get_attr(attr);
                let datum = template_from_expr(expr, field.type_desc(), recorder)?;
                if matches!(datum, TemplateDatum::Const(None)) && !field.nullable() {
                    return Err(mudu_error!(
                        ER::InvalidTuple,
                        format!("cannot insert NULL into NOT NULL column {}", field.name())
                    ));
                }
                if field.primary_index().is_some() {
                    match datum {
                        TemplateDatum::Const(None) => {
                            return Err(mudu_error!(
                                ER::InvalidTuple,
                                format!("cannot insert NULL into key column {}", field.name())
                            ))
                        }
                        datum => key.push((attr, datum)),
                    }
                } else if !matches!(datum, TemplateDatum::Const(None)) {
                    value.push((attr, datum));
                }
            }
            rows.push(InsertRowTemplate { key, value });
        }

        Ok(InsertTemplate {
            table_id: table_desc.id(),
            rows,
            has_fs_columns: has_fs_bound_columns(&table_desc),
        })
    }

    async fn bind_copy_from(
        &self,
        stmt: sql_parser::ast::stmt_copy_from::StmtCopyFrom,
    ) -> RS<BoundCopyFrom> {
        let table_desc = self.get_table_by_name(stmt.copy_to_table_name()).await?;
        let layout = CopyLayout::new(&table_desc, stmt.table_columns())?;
        Ok(BoundCopyFrom {
            file_path: stmt.copy_from_file_path().clone(),
            table_id: table_desc.id(),
            key_index: layout.key_index().to_vec(),
            value_index: layout.value_index().to_vec(),
        })
    }

    async fn bind_copy_to(
        &self,
        stmt: sql_parser::ast::stmt_copy_to::StmtCopyTo,
    ) -> RS<BoundCopyTo> {
        let table_desc = self.get_table_by_name(stmt.copy_from_table_name()).await?;
        let layout = CopyLayout::new(&table_desc, stmt.table_columns())?;
        Ok(BoundCopyTo {
            file_path: stmt.copy_to_file_path().clone(),
            table_id: table_desc.id(),
            key_indexing: layout.key_index().to_vec(),
            value_indexing: layout.value_index().to_vec(),
        })
    }

    async fn bind_update_template(
        &self,
        stmt: &StmtUpdate,
        recorder: &mut SlotRecorder,
    ) -> RS<UpdateTemplate> {
        let table_desc = self.get_table_by_name(stmt.get_table_reference()).await?;
        let mut value = Vec::with_capacity(stmt.get_set_values().len());

        for assignment in stmt.get_set_values() {
            let attr = self.attr_index_by_name(&table_desc, assignment.get_column_reference())?;
            let field = table_desc.get_attr(attr);
            if field.primary_index().is_some() {
                return Err(mudu_error!(
                    ER::NotImplemented,
                    "updating primary key columns is not implemented"
                ));
            }
            match assignment.get_set_value() {
                AssignedValue::Value(expr) => {
                    let datum = template_from_expr(expr, field.type_desc(), recorder)?;
                    if matches!(datum, TemplateDatum::Const(None)) && !field.nullable() {
                        return Err(mudu_error!(
                            ER::InvalidTuple,
                            format!("cannot update NOT NULL column {} to NULL", field.name())
                        ));
                    }
                    match datum {
                        // Assigning NULL to an FS-bound column marks it for rebinding:
                        // the DML hook assigns a fresh system object id (an empty
                        // datum is the "touched, system-assigned" sentinel).
                        TemplateDatum::Const(None) if field.fs_binding().is_some() => value.push((
                            attr,
                            SetValueTemplate::Absolute(TemplateDatum::Const(Some(Vec::new()))),
                        )),
                        TemplateDatum::Const(None) => {}
                        datum => value.push((attr, SetValueTemplate::Absolute(datum))),
                    }
                }
                AssignedValue::Expression(expr) => {
                    let set_value = Self::bind_delta_assignment_template(
                        assignment.get_column_reference(),
                        expr,
                        field.type_desc(),
                        recorder,
                    )?;
                    value.push((attr, set_value));
                }
            }
        }
        let key =
            self.bind_exact_key_template(&table_desc, stmt.get_where_predicate(), recorder)?;

        Ok(UpdateTemplate {
            table_id: table_desc.id(),
            key,
            value,
            has_fs_columns: has_fs_bound_columns(&table_desc),
        })
    }

    /// Bind the restricted expression assignment `SET col = col <+|->
    /// <integer>` where the right operand is an integer literal or a parameter
    /// placeholder (`?`).
    ///
    /// The left operand must reference the assigned column itself and the
    /// right operand must be a signed/unsigned integer literal or a parameter
    /// of an integer type family; the assigned column must be an integer or
    /// numeric column. Any other expression form is rejected as
    /// `NotImplemented`. The operand is stored in the column's binary format
    /// (coerced for numeric columns) so the executor can decode it with the
    /// column type. A placeholder is recorded as a delta-operand slot; its
    /// type-family check and encoding run at fill time through the same path
    /// as a plain `SET col = ?` assignment (identical to immediate binding,
    /// where both run at bind time).
    fn bind_delta_assignment_template(
        column_reference: &str,
        expr: &ExprType,
        column_type: &DataType,
        recorder: &mut SlotRecorder,
    ) -> RS<SetValueTemplate> {
        let not_implemented = || {
            mudu_error!(
                ER::NotImplemented,
                "expression updates are not implemented \
                 (only `SET col = col +|- <integer literal or ?>` is supported)"
            )
        };
        let ExprType::Arithmetic(arithmetic) = expr else {
            return Err(not_implemented());
        };
        let op = match arithmetic.op() {
            Arithmetic::PLUS => DeltaOp::Add,
            Arithmetic::MINUS => DeltaOp::Sub,
            _ => return Err(not_implemented()),
        };
        let ExprType::Value(left) = arithmetic.left() else {
            return Err(not_implemented());
        };
        let ExprItem::ItemName(name) = left.as_ref() else {
            return Err(not_implemented());
        };
        if name.name() != column_reference {
            return Err(not_implemented());
        }
        match column_type.type_family() {
            // Numeric columns (e.g. TPC-C money columns) accept an integer
            // delta operand; the operand is coerced to the column's numeric
            // layout when encoded.
            TypeFamily::I32
            | TypeFamily::I64
            | TypeFamily::I128
            | TypeFamily::U128
            | TypeFamily::Numeric => {}
            _ => return Err(not_implemented()),
        }
        let ExprType::Value(right) = arithmetic.right() else {
            return Err(not_implemented());
        };
        let ExprItem::ItemValue(value) = right.as_ref() else {
            return Err(not_implemented());
        };
        let operand = match value {
            ExprValue::ValueLiteral(ExprLiteral::DatumLiteral(literal)) => {
                match literal.data_type().type_family() {
                    TypeFamily::I32 | TypeFamily::I64 | TypeFamily::I128 | TypeFamily::U128 => {}
                    // Drivers that substitute params client-side send numeric
                    // delta operands as quoted string literals; ValueCodec
                    // coerces them to the column's numeric layout.
                    TypeFamily::String if column_type.type_family() == TypeFamily::Numeric => {}
                    _ => return Err(not_implemented()),
                }
                let Some(binary) = ValueCodec::binary_from_literal(
                    &ExprLiteral::DatumLiteral(literal.clone()),
                    column_type,
                )?
                else {
                    // A datum literal never binds to NULL.
                    return Err(not_implemented());
                };
                TemplateDatum::Const(Some(binary))
            }
            ExprValue::ValuePlaceholder => {
                // The placeholder family check (integer families, or String
                // for numeric columns whose params arrive type-erased as
                // strings) runs at fill time in `fill_slot`, which applies
                // the same rules and then encodes through the same path as a
                // plain `SET col = ?` assignment.
                TemplateDatum::Slot(recorder.push(column_type.clone(), true))
            }
            _ => return Err(not_implemented()),
        };
        Ok(SetValueTemplate::Delta { op, operand })
    }

    async fn bind_delete_template(
        &self,
        stmt: &StmtDelete,
        recorder: &mut SlotRecorder,
    ) -> RS<DeleteTemplate> {
        let table_desc = self.get_table_by_name(stmt.get_table_reference()).await?;
        let key =
            self.bind_exact_key_template(&table_desc, stmt.get_where_predicate(), recorder)?;
        Ok(DeleteTemplate {
            table_id: table_desc.id(),
            key,
        })
    }

    fn bind_predicate_template(
        &self,
        table_desc: &TableDesc,
        predicates: &[ExprCompare],
        recorder: &mut SlotRecorder,
    ) -> RS<(PredicateTemplate, Vec<ResidualTemplate>)> {
        if predicates.is_empty() {
            return Ok((PredicateTemplate::True, Vec::new()));
        }

        let mut eq_items = vec![];
        let mut start: Bound<Vec<(usize, TemplateDatum)>> = Bound::Unbounded;
        let mut end: Bound<Vec<(usize, TemplateDatum)>> = Bound::Unbounded;
        let mut residual = Vec::new();

        for predicate in predicates {
            let (field_name, expr_value, op) =
                self.field_literal_compare(predicate).ok_or_else(|| {
                    mudu_error!(
                        ER::NotImplemented,
                        "only column/literal predicates are supported"
                    )
                })?;
            let attr = self.attr_index_by_name(table_desc, field_name)?;
            let field = table_desc.get_attr(attr);
            let datum = template_from_expr(&expr_value, field.type_desc(), recorder)?;
            if field.primary_index().is_none() {
                // Non-key predicate: evaluate it row-by-row in the executor
                // layer as a residual filter after the key access.
                residual.push(ResidualTemplate {
                    attr,
                    op,
                    literal: datum,
                });
                continue;
            }
            if matches!(datum, TemplateDatum::Const(None)) {
                return Err(mudu_error!(
                    ER::NotImplemented,
                    "NULL key predicates are not implemented; use IS NULL"
                ));
            }
            match op {
                ValueCompare::EQ => eq_items.push((attr, datum)),
                ValueCompare::GE => start = Bound::Included(vec![(attr, datum)]),
                ValueCompare::GT => start = Bound::Excluded(vec![(attr, datum)]),
                ValueCompare::LE => end = Bound::Included(vec![(attr, datum)]),
                ValueCompare::LT => end = Bound::Excluded(vec![(attr, datum)]),
                ValueCompare::NE => {
                    return Err(mudu_error!(
                        ER::NotImplemented,
                        "not-equal predicates are not implemented"
                    ))
                }
            }
        }

        let predicate = self.combine_key_predicate_template(table_desc, eq_items, start, end)?;
        Ok((predicate, residual))
    }

    fn combine_key_predicate_template(
        &self,
        table_desc: &TableDesc,
        mut eq_items: Vec<(usize, TemplateDatum)>,
        start: Bound<Vec<(usize, TemplateDatum)>>,
        end: Bound<Vec<(usize, TemplateDatum)>>,
    ) -> RS<PredicateTemplate> {
        if !eq_items.is_empty()
            && matches!(start, Bound::Unbounded)
            && matches!(end, Bound::Unbounded)
        {
            eq_items.sort_by_key(|(attr, _)| {
                table_desc
                    .get_attr(*attr)
                    .primary_index()
                    .unwrap_or_default()
            });
            for (index, (attr, _)) in eq_items.iter().enumerate() {
                if table_desc.get_attr(*attr).primary_index() != Some(index) {
                    return Err(mudu_error!(
                        ER::NotImplemented,
                        "select equality predicates on primary keys must cover a left prefix of the primary key"
                    ));
                }
            }
            if eq_items.len() == table_desc.key_indices().len() {
                return Ok(PredicateTemplate::KeyEq { key: eq_items });
            }
            return Ok(PredicateTemplate::KeyPrefixEq { prefix: eq_items });
        }

        if !eq_items.is_empty() {
            return Err(mudu_error!(
                ER::NotImplemented,
                "mixed equality and range predicates are not implemented"
            ));
        }

        Ok(PredicateTemplate::KeyRange { start, end })
    }

    fn bind_exact_key_template(
        &self,
        table_desc: &TableDesc,
        predicates: &[ExprCompare],
        recorder: &mut SlotRecorder,
    ) -> RS<Vec<(usize, TemplateDatum)>> {
        let (predicate, residual) =
            self.bind_predicate_template(table_desc, predicates, recorder)?;
        if !residual.is_empty() {
            return Err(mudu_error!(
                ER::NotImplemented,
                "non-key predicates are not implemented"
            ));
        }
        match predicate {
            PredicateTemplate::KeyEq { mut key } => {
                if key.len() != table_desc.key_indices().len() {
                    return Err(mudu_error!(
                        ER::NotImplemented,
                        "update/delete require a complete primary key predicate"
                    ));
                }
                key.sort_by_key(|(attr, _)| {
                    table_desc
                        .get_attr(*attr)
                        .primary_index()
                        .unwrap_or_default()
                });
                for (index, (attr, _)) in key.iter().enumerate() {
                    if table_desc.get_attr(*attr).primary_index() != Some(index) {
                        return Err(mudu_error!(
                            ER::NotImplemented,
                            "update/delete require one equality predicate for each primary key column"
                        ));
                    }
                }
                Ok(key)
            }
            PredicateTemplate::KeyPrefixEq { .. } => Err(mudu_error!(
                ER::NotImplemented,
                "update/delete require a complete primary key predicate"
            )),
            PredicateTemplate::True => Err(mudu_error!(
                ER::NotImplemented,
                "full-table update/delete is not implemented"
            )),
            PredicateTemplate::KeyRange { .. } => Err(mudu_error!(
                ER::NotImplemented,
                "range update/delete is not implemented"
            )),
        }
    }

    fn field_literal_compare<'a>(
        &self,
        predicate: &'a ExprCompare,
    ) -> Option<(&'a String, ExprValue, ValueCompare)> {
        match (predicate.left(), predicate.right()) {
            (ExprItem::ItemName(name), ExprItem::ItemValue(value)) => {
                Some((name.name(), value.clone(), *predicate.op()))
            }
            (ExprItem::ItemValue(value), ExprItem::ItemName(name)) => Some((
                name.name(),
                value.clone(),
                Self::reverse_compare(*predicate.op()),
            )),
            _ => None,
        }
    }

    fn reverse_compare(op: ValueCompare) -> ValueCompare {
        ValueCompare::revert_cmp_op(op)
    }

    async fn schema_column_from_ast(
        &self,
        column: &sql_parser::ast::column_def::ColumnDef,
    ) -> RS<SchemaColumn> {
        let mut schema_column = match column.data_type().as_identifier() {
            Some(name) => self.schema_column_from_named_type(column, name).await?,
            None => {
                let ty = column
                    .data_type()
                    .clone()
                    .uni_to_with_params(column.data_type_param().clone())?;
                SchemaColumn::new(
                    column.column_name().clone(),
                    ty.type_family(),
                    DataTypeInfo::from_opt_object(&ty),
                )
            }
        };
        schema_column.set_primary_index(column.primary_key_index());
        schema_column.set_nullable(column.nullable());
        schema_column.set_index(column.column_index());
        Ok(schema_column)
    }

    // Resolve a column declared with a named type (e.g. a registered filesystem
    // type) to its physical U128 storage column and record the type binding.
    async fn schema_column_from_named_type(
        &self,
        column: &sql_parser::ast::column_def::ColumnDef,
        name: &str,
    ) -> RS<SchemaColumn> {
        let desc = self
            .meta_mgr
            .get_fs_type_by_name(name)
            .await?
            .ok_or_else(|| {
                mudu_error!(ER::EntityNotFound, format!("unknown type name {}", name))
            })?;
        let ty = DataType::new_no_param(TypeFamily::U128);
        let mut schema_column = SchemaColumn::new(
            column.column_name().clone(),
            ty.type_family(),
            DataTypeInfo::from_opt_object(&ty),
        );
        schema_column.set_fs_binding(Some(FsColumnBinding::new(desc.fs_id(), desc.kind())));
        Ok(schema_column)
    }

    fn attr_index_by_name(&self, table_desc: &TableDesc, name: &str) -> RS<usize> {
        let total = table_desc.fields().len();
        (0..total)
            .find(|attr| table_desc.get_attr(*attr).name() == name)
            .ok_or_else(|| mudu_error!(ER::EntityNotFound, format!("cannot find column {}", name)))
    }

    async fn get_table_by_name(&self, name: &str) -> RS<Arc<TableDesc>> {
        self.meta_mgr
            .get_table_by_name(name)
            .await?
            .ok_or_else(|| mudu_error!(ER::EntityNotFound, format!("no such table {}", name)))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InferredKeyType {
    I64,
    F64,
    String,
}

impl InferredKeyType {
    fn merge(self, next: InferredKeyType) -> InferredKeyType {
        use InferredKeyType::*;
        match (self, next) {
            (String, _) | (_, String) => String,
            (F64, _) | (_, F64) => F64,
            _ => I64,
        }
    }

    fn to_type_family(self) -> TypeFamily {
        match self {
            InferredKeyType::I64 => TypeFamily::I64,
            InferredKeyType::F64 => TypeFamily::F64,
            InferredKeyType::String => TypeFamily::String,
        }
    }
}

fn infer_textual_value_type(raw: &[u8]) -> RS<InferredKeyType> {
    let text = String::from_utf8(raw.to_vec())
        .map_err(|e| mudu_error!(ER::Decode, "partition bound text is not utf8", e))?;
    let text = strip_text_literal_quotes(text.trim());
    if text.parse::<i64>().is_ok() {
        return Ok(InferredKeyType::I64);
    }
    if text.parse::<f64>().is_ok() {
        return Ok(InferredKeyType::F64);
    }
    Ok(InferredKeyType::String)
}

fn strip_text_literal_quotes(input: &str) -> String {
    if input.len() >= 2 && input.starts_with('\'') && input.ends_with('\'') {
        input[1..input.len() - 1].to_string()
    } else {
        input.to_string()
    }
}
