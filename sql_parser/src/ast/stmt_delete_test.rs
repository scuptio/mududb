//! Unit tests for `StmtDelete`.

#![allow(missing_docs)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::panic)]

use crate::ast::expr_compare::ExprCompare;
use crate::ast::expr_item::{ExprItem, ExprValue};
use crate::ast::expr_literal::ExprLiteral;
use crate::ast::expr_name::ExprName;
use crate::ast::expr_operator::ValueCompare;
use crate::ast::stmt_delete::StmtDelete;
use mudu_type::data_typed::DataTyped;

fn sample_predicate() -> ExprCompare {
    let mut name = ExprName::new();
    name.set_name("id".to_string());
    let left = ExprItem::ItemName(name);
    let right = ExprItem::ItemValue(ExprValue::ValueLiteral(ExprLiteral::DatumLiteral(
        DataTyped::from_i32(1),
    )));
    ExprCompare::new(ValueCompare::EQ, left, right)
}

#[test]
fn new_creates_empty_delete_statement() {
    let stmt = StmtDelete::new();
    assert!(stmt.get_table_reference().is_empty());
    assert!(stmt.get_where_predicate().is_empty());
}

#[test]
fn default_creates_empty_delete_statement() {
    let stmt = StmtDelete::default();
    assert!(stmt.get_table_reference().is_empty());
}

#[test]
fn set_table_reference_updates_name() {
    let mut stmt = StmtDelete::new();
    stmt.set_table_reference("users".to_string());
    assert_eq!(stmt.get_table_reference(), "users");
}

#[test]
fn add_and_replace_where_predicates() {
    let mut stmt = StmtDelete::new();
    stmt.add_where_predicate(sample_predicate());
    assert_eq!(stmt.get_where_predicate().len(), 1);

    stmt.set_where_predicate(vec![]);
    assert!(stmt.get_where_predicate().is_empty());
}
