//! Parser tests for the `sql_parser` crate.
//!
//! Miri cannot execute FFI calls into the tree-sitter C parser, so tests that
//! depend on the parser are skipped under Miri.

#![allow(missing_docs)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::panic)]

mod create_table;
mod ddl_partition;
mod dml;

use crate::ast::expr_item::{ExprItem, ExprValue};
use crate::ast::expr_operator::{Arithmetic, ValueCompare};
use crate::ast::expression::ExprType;
use crate::ast::parser::SQLParser;
use crate::ast::stmt_create_table::StmtCreateTable;
use crate::ast::stmt_type::{StmtCommand, StmtType};
use crate::ast::stmt_update::AssignedValue;
use mudu::common::result::RS;
use mudu_binding::universal::uni_data_type::UniDataType;
use mudu_binding::universal::uni_data_value::UniDataValue;
use mudu_binding::universal::uni_scalar::UniScalar;
use mudu_binding::universal::uni_scalar_value::UniScalarValue;

fn parse_sql(sql: &str) -> RS<Vec<StmtType>> {
    let parser = SQLParser::new()?;
    Ok(parser.parse(sql)?.stmts().clone())
}

fn parse_create_table(sql: &str) -> RS<StmtCreateTable> {
    let stmts = parse_sql(sql)?;
    let stmt = stmts.first().ok_or_else(|| {
        mudu::mudu_error!(mudu::error::ErrorCode::Parse, "expected one statement")
    })?;
    match stmt {
        StmtType::Command(StmtCommand::CreateTable(stmt)) => Ok(stmt.clone()),
        _ => Err(mudu::mudu_error!(
            mudu::error::ErrorCode::Parse,
            "expected create table statement"
        )),
    }
}
