use crate::contract::meta_mgr::MetaMgr;
use crate::contract::table_desc::TableDesc;
use mudu::common::result::RS;
use mudu::error::ErrorCode as ER;
use mudu::mudu_error;
use mudu_contract::tuple::tuple_field_desc::TupleFieldDesc;
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

    pub async fn describe(meta_mgr: &dyn MetaMgr, stmt: &StmtType) -> RS<TupleFieldDesc> {
        match stmt {
            StmtType::Select(stmt) => Self::describe_select(meta_mgr, stmt).await,
            StmtType::Command(_) => Ok(TupleFieldDesc::new(Vec::new())),
        }
    }

    async fn describe_select(
        meta_mgr: &dyn MetaMgr,
        stmt: &sql_parser::ast::stmt_select::StmtSelect,
    ) -> RS<TupleFieldDesc> {
        let table_desc = Self::get_table_by_name(meta_mgr, stmt.get_table_reference()).await?;
        let (_items, tuple_desc) = crate::sql::select_projection::bind_select_items(
            &table_desc,
            stmt.get_select_term_list(),
        )?;
        Ok(tuple_desc)
    }

    async fn get_table_by_name(meta_mgr: &dyn MetaMgr, name: &str) -> RS<Arc<TableDesc>> {
        meta_mgr
            .get_table_by_name(name)
            .await?
            .ok_or_else(|| mudu_error!(ER::EntityNotFound, format!("no such table {}", name)))
    }
}
