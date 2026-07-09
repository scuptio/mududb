use crate::contract::meta_mgr::MetaMgr;
use crate::contract::partition_rule::{PartitionBound, PartitionRuleDesc, RangePartitionDef};
use crate::contract::partition_rule_binding::{PartitionPlacement, TablePartitionBinding};
use crate::contract::schema_column::SchemaColumn;
use crate::contract::schema_table::SchemaTable;
use crate::contract::table_desc::TableDesc;
use crate::executor::project_tuple_desc;
use crate::sql::bound_stmt::{
    BoundCommand, BoundCopyFrom, BoundCopyTo, BoundCreatePartitionPlacement,
    BoundCreatePartitionRule, BoundCreateTable, BoundDelete, BoundDropTable, BoundInsert,
    BoundInsertRow, BoundPredicate, BoundQuery, BoundSelect, BoundStmt, BoundUpdate,
};
use crate::sql::copy_layout::CopyLayout;
use crate::sql::value_codec::ValueCodec;
use mudu::common::result::RS;
use mudu::error::ErrorCode as ER;
use mudu::mudu_error;
use mudu_contract::database::sql_params::SQLParams;
use mudu_type::data_type_info::DataTypeInfo;
use mudu_type::type_family::TypeFamily;
use sql_parser::ast::expr_compare::ExprCompare;
use sql_parser::ast::expr_item::{ExprItem, ExprValue};
use sql_parser::ast::expr_operator::ValueCompare;
use sql_parser::ast::stmt_create_partition_placement::StmtCreatePartitionPlacement;
use sql_parser::ast::stmt_create_partition_rule::{StmtCreatePartitionRule, StmtPartitionBound};
use sql_parser::ast::stmt_create_table::StmtCreateTable;
use sql_parser::ast::stmt_delete::StmtDelete;
use sql_parser::ast::stmt_drop_table::StmtDropTable;
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
        match stmt {
            StmtType::Select(stmt) => Ok(BoundStmt::Query(BoundQuery::Select(
                self.bind_select(stmt, params).await?,
            ))),
            StmtType::Command(command) => Ok(BoundStmt::Command(
                self.bind_command(command, params).await?,
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
            StmtCommand::Insert(stmt) => {
                Ok(BoundCommand::Insert(self.bind_insert(stmt, params).await?))
            }
            StmtCommand::Update(stmt) => {
                Ok(BoundCommand::Update(self.bind_update(stmt, params).await?))
            }
            StmtCommand::Delete(stmt) => {
                Ok(BoundCommand::Delete(self.bind_delete(stmt, params).await?))
            }
            StmtCommand::CopyFrom(stmt) => {
                Ok(BoundCommand::CopyFrom(self.bind_copy_from(stmt).await?))
            }
            StmtCommand::CopyTo(stmt) => Ok(BoundCommand::CopyTo(self.bind_copy_to(stmt).await?)),
        }
    }

    async fn bind_select(
        &self,
        stmt: sql_parser::ast::stmt_select::StmtSelect,
        params: &dyn SQLParams,
    ) -> RS<BoundSelect> {
        let table_desc = self.get_table_by_name(stmt.get_table_reference()).await?;
        let select_attrs = self.select_attrs(&table_desc, stmt.get_select_term_list())?;
        let tuple_desc = project_tuple_desc(
            &table_desc,
            &crate::x_engine::api::VecSelTerm::new(select_attrs.clone()),
        );
        let predicate = self.bind_predicate(&table_desc, stmt.get_where_predicate(), params)?;
        Ok(BoundSelect {
            table_id: table_desc.id(),
            select_attrs,
            tuple_desc,
            predicate,
        })
    }

    async fn bind_create_table(&self, mut stmt: StmtCreateTable) -> RS<BoundCreateTable> {
        stmt.assign_index_for_columns();
        let key_columns = stmt
            .primary_columns()
            .into_iter()
            .map(Self::schema_column_from_ast)
            .collect::<RS<Vec<_>>>()?;
        let value_columns = stmt
            .non_primary_columns()
            .into_iter()
            .map(Self::schema_column_from_ast)
            .collect::<RS<Vec<_>>>()?;
        let mut columns = key_columns;
        let value_offset = columns.len();
        let mut value_columns = value_columns;
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

    async fn bind_insert(&self, stmt: StmtInsert, params: &dyn SQLParams) -> RS<BoundInsert> {
        let table_desc = self.get_table_by_name(stmt.table_name()).await?;

        let columns = if stmt.columns().is_empty() {
            let total = table_desc.fields().len();
            (0..total)
                .map(|attr| table_desc.get_attr(attr).name().clone())
                .collect::<Vec<_>>()
        } else {
            stmt.columns().clone()
        };

        let mut param_index = 0;
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
                let binary = ValueCodec::binary_from_expr(
                    expr,
                    field.type_desc(),
                    params,
                    &mut param_index,
                )?;
                if binary.is_none() && !field.nullable() {
                    return Err(mudu_error!(
                        ER::InvalidTuple,
                        format!("cannot insert NULL into NOT NULL column {}", field.name())
                    ));
                }
                if field.primary_index().is_some() {
                    let binary = binary.ok_or_else(|| {
                        mudu_error!(
                            ER::InvalidTuple,
                            format!("cannot insert NULL into key column {}", field.name())
                        )
                    })?;
                    key.push((attr, binary));
                } else if let Some(binary) = binary {
                    value.push((attr, binary));
                }
            }
            rows.push(BoundInsertRow { key, value });
        }

        Ok(BoundInsert {
            table_id: table_desc.id(),
            rows,
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

    async fn bind_update(&self, stmt: StmtUpdate, params: &dyn SQLParams) -> RS<BoundUpdate> {
        let table_desc = self.get_table_by_name(stmt.get_table_reference()).await?;
        let mut param_index = 0;
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
            let AssignedValue::Value(expr) = assignment.get_set_value() else {
                return Err(mudu_error!(
                    ER::NotImplemented,
                    "expression updates are not implemented"
                ));
            };
            let binary =
                ValueCodec::binary_from_expr(expr, field.type_desc(), params, &mut param_index)?;
            if binary.is_none() && !field.nullable() {
                return Err(mudu_error!(
                    ER::InvalidTuple,
                    format!("cannot update NOT NULL column {} to NULL", field.name())
                ));
            }
            if let Some(binary) = binary {
                value.push((attr, binary));
            }
        }
        let key = self.bind_exact_key_from(
            &table_desc,
            stmt.get_where_predicate(),
            params,
            &mut param_index,
        )?;

        Ok(BoundUpdate {
            table_id: table_desc.id(),
            key,
            value,
        })
    }

    async fn bind_delete(&self, stmt: StmtDelete, params: &dyn SQLParams) -> RS<BoundDelete> {
        let table_desc = self.get_table_by_name(stmt.get_table_reference()).await?;
        let key = self.bind_exact_key(&table_desc, stmt.get_where_predicate(), params)?;
        Ok(BoundDelete {
            table_id: table_desc.id(),
            key,
        })
    }

    fn bind_predicate(
        &self,
        table_desc: &TableDesc,
        predicates: &[ExprCompare],
        params: &dyn SQLParams,
    ) -> RS<BoundPredicate> {
        let mut param_index = 0;
        self.bind_predicate_from(table_desc, predicates, params, &mut param_index)
    }

    fn bind_predicate_from(
        &self,
        table_desc: &TableDesc,
        predicates: &[ExprCompare],
        params: &dyn SQLParams,
        param_index: &mut usize,
    ) -> RS<BoundPredicate> {
        if predicates.is_empty() {
            return Ok(BoundPredicate::True);
        }

        let mut eq_items = vec![];
        let mut start: Bound<Vec<(usize, Vec<u8>)>> = Bound::Unbounded;
        let mut end: Bound<Vec<(usize, Vec<u8>)>> = Bound::Unbounded;

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
            if field.primary_index().is_none() {
                return Err(mudu_error!(
                    ER::NotImplemented,
                    "non-key predicates are not implemented"
                ));
            }
            let binary =
                ValueCodec::binary_from_expr(&expr_value, field.type_desc(), params, param_index)?;
            let binary = binary.ok_or_else(|| {
                mudu_error!(
                    ER::NotImplemented,
                    "NULL key predicates are not implemented; use IS NULL"
                )
            })?;
            match op {
                ValueCompare::EQ => eq_items.push((attr, binary)),
                ValueCompare::GE => start = Bound::Included(vec![(attr, binary)]),
                ValueCompare::GT => start = Bound::Excluded(vec![(attr, binary)]),
                ValueCompare::LE => end = Bound::Included(vec![(attr, binary)]),
                ValueCompare::LT => end = Bound::Excluded(vec![(attr, binary)]),
                ValueCompare::NE => {
                    return Err(mudu_error!(
                        ER::NotImplemented,
                        "not-equal predicates are not implemented"
                    ))
                }
            }
        }

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
                return Ok(BoundPredicate::KeyEq { key: eq_items });
            }
            return Ok(BoundPredicate::KeyPrefixEq { prefix: eq_items });
        }

        if !eq_items.is_empty() {
            return Err(mudu_error!(
                ER::NotImplemented,
                "mixed equality and range predicates are not implemented"
            ));
        }

        Ok(BoundPredicate::KeyRange { start, end })
    }

    fn bind_exact_key(
        &self,
        table_desc: &TableDesc,
        predicates: &[ExprCompare],
        params: &dyn SQLParams,
    ) -> RS<Vec<(usize, Vec<u8>)>> {
        let mut param_index = 0;
        self.bind_exact_key_from(table_desc, predicates, params, &mut param_index)
    }

    fn bind_exact_key_from(
        &self,
        table_desc: &TableDesc,
        predicates: &[ExprCompare],
        params: &dyn SQLParams,
        param_index: &mut usize,
    ) -> RS<Vec<(usize, Vec<u8>)>> {
        match self.bind_predicate_from(table_desc, predicates, params, param_index)? {
            BoundPredicate::KeyEq { mut key } => {
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
            BoundPredicate::KeyPrefixEq { .. } => Err(mudu_error!(
                ER::NotImplemented,
                "update/delete require a complete primary key predicate"
            )),
            BoundPredicate::True => Err(mudu_error!(
                ER::NotImplemented,
                "full-table update/delete is not implemented"
            )),
            BoundPredicate::KeyRange { .. } => Err(mudu_error!(
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

    fn schema_column_from_ast(column: &sql_parser::ast::column_def::ColumnDef) -> RS<SchemaColumn> {
        let ty = column
            .data_type()
            .clone()
            .uni_to_with_params(column.data_type_param().clone())?;
        let mut schema_column = SchemaColumn::new(
            column.column_name().clone(),
            ty.type_family(),
            DataTypeInfo::from_opt_object(&ty),
        );
        schema_column.set_primary_index(column.primary_key_index());
        schema_column.set_nullable(column.nullable());
        schema_column.set_index(column.column_index());
        Ok(schema_column)
    }

    fn select_attrs(
        &self,
        table_desc: &TableDesc,
        terms: &[sql_parser::ast::select_term::SelectTerm],
    ) -> RS<Vec<usize>> {
        terms
            .iter()
            .map(|term| self.attr_index_by_name(table_desc, term.field().name()))
            .collect()
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
