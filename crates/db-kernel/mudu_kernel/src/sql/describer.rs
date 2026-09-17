use crate::contract::meta_mgr::MetaMgr;
use crate::contract::table_desc::TableDesc;
use mudu::common::result::RS;
use mudu::error::ErrorCode as ER;
use mudu::mudu_error;
use mudu_contract::tuple::tuple_field_desc::TupleFieldDesc;
use sql_parser::ast::object_ref::ObjectRef;
use sql_parser::ast::stmt_type::StmtType;
use std::sync::Arc;

pub struct Describer {}

impl Default for Describer {
    fn default() -> Self {
        Self::new()
    }
}

impl Describer {
    pub fn new() -> Self {
        Self {}
    }

    /// Describe the result shape of `stmt`; unqualified table references
    /// resolve against `current_schema` (search path `[current_schema,
    /// mududb]`), qualified references resolve exactly.
    pub async fn describe(
        meta_mgr: &dyn MetaMgr,
        stmt: &StmtType,
        current_schema: &str,
    ) -> RS<TupleFieldDesc> {
        match stmt {
            StmtType::Select(stmt) => Self::describe_select(meta_mgr, stmt, current_schema).await,
            StmtType::Command(_) => Ok(TupleFieldDesc::new(Vec::new())),
        }
    }

    async fn describe_select(
        meta_mgr: &dyn MetaMgr,
        stmt: &sql_parser::ast::stmt_select::StmtSelect,
        current_schema: &str,
    ) -> RS<TupleFieldDesc> {
        let table_desc =
            Self::resolve_table(meta_mgr, stmt.get_table_reference(), current_schema).await?;
        let (_items, tuple_desc) = crate::sql::select_projection::bind_select_items(
            &table_desc,
            stmt.get_select_term_list(),
        )?;
        Ok(tuple_desc)
    }

    async fn resolve_table(
        meta_mgr: &dyn MetaMgr,
        reference: &ObjectRef,
        current_schema: &str,
    ) -> RS<Arc<TableDesc>> {
        let opt = match reference.schema() {
            Some(schema) => meta_mgr.get_table_exact(schema, reference.name()).await?,
            None => {
                meta_mgr
                    .get_table_by_name(current_schema, reference.name())
                    .await?
            }
        };
        opt.ok_or_else(|| {
            mudu_error!(
                ER::EntityNotFound,
                format!("no such table {}", reference.name())
            )
        })
    }
}
