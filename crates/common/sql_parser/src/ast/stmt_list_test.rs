//! Unit tests for `StmtList`.

#![allow(missing_docs)]
#![allow(clippy::unwrap_used)]

use crate::ast::stmt_create_table::StmtCreateTable;
use crate::ast::stmt_list::StmtList;
use crate::ast::stmt_type::{StmtCommand, StmtType};

fn create_table_stmt(name: &str) -> StmtType {
    StmtType::Command(StmtCommand::CreateTable(StmtCreateTable::new(
        name.to_string(),
    )))
}

#[test]
fn stmts_returns_statements_in_order() {
    let stmt1 = create_table_stmt("t1");
    let stmt2 = create_table_stmt("t2");
    let list = StmtList::new(vec![stmt1.clone(), stmt2.clone()]);
    let stmts = list.stmts();
    assert_eq!(stmts.len(), 2);
    assert_eq!(format!("{:?}", stmts[0]), format!("{:?}", stmt1));
    assert_eq!(format!("{:?}", stmts[1]), format!("{:?}", stmt2));
}

#[test]
fn into_stmts_consumes_self_and_returns_owned_vec() {
    let stmt1 = create_table_stmt("t1");
    let stmt2 = create_table_stmt("t2");
    let list = StmtList::new(vec![stmt1.clone(), stmt2.clone()]);
    let stmts = list.into_stmts();
    assert_eq!(stmts.len(), 2);
    assert_eq!(format!("{:?}", stmts[0]), format!("{:?}", stmt1));
    assert_eq!(format!("{:?}", stmts[1]), format!("{:?}", stmt2));
}

#[test]
fn empty_list_is_empty() {
    let list = StmtList::new(vec![]);
    assert!(list.stmts().is_empty());
    assert_eq!(format!("{:?}", list), "");
    assert!(list.into_stmts().is_empty());
}

#[test]
fn debug_fmt_separates_two_statements_with_newline() {
    let stmt1 = create_table_stmt("t1");
    let stmt2 = create_table_stmt("t2");
    let list = StmtList::new(vec![stmt1.clone(), stmt2.clone()]);
    let output = format!("{:?}", list);
    let expected = format!("{:?}\n{:?}", stmt1, stmt2);
    assert_eq!(output, expected);
}

#[test]
fn debug_fmt_single_statement_has_no_trailing_newline() {
    let stmt = create_table_stmt("t1");
    let list = StmtList::new(vec![stmt.clone()]);
    let output = format!("{:?}", list);
    assert_eq!(output, format!("{:?}", stmt));
    assert!(!output.ends_with('\n'));
}
