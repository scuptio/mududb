use crate::bound_stmt::{
    BoundCommand, BoundCopyFrom, BoundCopyTo, BoundCreateFsType, BoundCreatePartitionPlacement,
    BoundCreatePartitionRule, BoundCreateTable, BoundDropTable, BoundDropType, BoundStmt,
};
use crate::bound_template::{
    template_from_expr, BoundTemplate, DeleteTemplate, InsertRowTemplate, InsertTemplate,
    ParamSlot, PredicateTemplate, ResidualTemplate, SelectTemplate, SetValueTemplate, SlotRecorder,
    StmtTemplate, TemplateDatum, UpdateTemplate,
};
use crate::copy_layout::CopyLayout;
use crate::value_codec::ValueCodec;
use ::catalog::contract::fs_type::{FsColumnBinding, FsTypeKind};
use ::catalog::contract::meta_mgr::MetaMgr;
use ::catalog::contract::partition_rule::{PartitionBound, PartitionRuleDesc, RangePartitionDef};
use ::catalog::contract::partition_rule_binding::{PartitionPlacement, TablePartitionBinding};
use ::catalog::contract::schema_column::SchemaColumn;
use ::catalog::contract::schema_table::SchemaTable;
use ::catalog::contract::table_desc::TableDesc;
use ::command::fs_hook::has_fs_bound_columns;
use ::xengine::api::DeltaOp;
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
use sql_parser::ast::object_ref::ObjectRef;
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

mod ddl;
mod dml;

/// Binds parsed SQL statements against the catalog, producing bound
/// statement trees with table OIDs and binary-encoded values.
pub struct Binder {
    meta_mgr: Arc<dyn MetaMgr>,
    // Schema that unqualified table references resolve against (and that
    // unqualified CREATE TABLE lands in); qualified `schema.table`
    // references bypass it and resolve exactly.
    current_schema: String,
}

impl Binder {
    /// Creates a binder that resolves catalog objects through `meta_mgr`;
    /// unqualified table references resolve against `current_schema`.
    pub fn new(meta_mgr: Arc<dyn MetaMgr>, current_schema: String) -> Self {
        Self {
            meta_mgr,
            current_schema,
        }
    }

    /// Binds `stmt`, filling its placeholders from `params`.
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

    /// Resolve a statement's table reference: a qualified `schema.table`
    /// reference hits exactly `(schema, table)` with no search path; an
    /// unqualified reference resolves through the search path rooted at the
    /// current schema (`[current_schema, mududb]` inside the catalog).
    async fn resolve_table_opt(&self, reference: &ObjectRef) -> RS<Option<Arc<TableDesc>>> {
        match reference.schema() {
            Some(schema) => {
                self.meta_mgr
                    .get_table_exact(schema, reference.name())
                    .await
            }
            None => {
                self.meta_mgr
                    .get_table_by_name(&self.current_schema, reference.name())
                    .await
            }
        }
    }

    async fn resolve_table(&self, reference: &ObjectRef) -> RS<Arc<TableDesc>> {
        self.resolve_table_opt(reference).await?.ok_or_else(|| {
            mudu_error!(
                ER::EntityNotFound,
                format!("no such table {}", reference.name())
            )
        })
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
