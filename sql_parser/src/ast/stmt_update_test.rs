//! Unit tests for `StmtUpdate`.

#![allow(missing_docs)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::panic)]

use crate::ast::expr_compare::ExprCompare;
use crate::ast::expr_item::{ExprItem, ExprValue};
use crate::ast::expr_literal::ExprLiteral;
use crate::ast::expr_name::ExprName;
use crate::ast::expr_operator::ValueCompare;
use crate::ast::expression::ExprType;
use crate::ast::stmt_update::{AssignedValue, Assignment, StmtUpdate};
use mudu_type::data_typed::DataTyped;
use std::sync::Arc;

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
fn assignment_stores_column_and_value() {
    let value = AssignedValue::Value(ExprValue::ValueLiteral(ExprLiteral::DatumLiteral(
        DataTyped::from_i32(42),
    )));
    let mut assignment = Assignment::new("balance".to_string(), value);
    assert_eq!(assignment.get_column_reference(), "balance");
    assert!(matches!(
        assignment.get_set_value(),
        AssignedValue::Value(_)
    ));

    let new_value = AssignedValue::Value(ExprValue::ValueLiteral(ExprLiteral::DatumLiteral(
        DataTyped::from_i32(99),
    )));
    assignment.set_set_value(new_value);
    assert!(matches!(
        assignment.get_set_value(),
        AssignedValue::Value(_)
    ));

    assignment.set_column_reference("amount".to_string());
    assert_eq!(assignment.get_column_reference(), "amount");
}

#[test]
fn update_statement_accessors_and_mutators() {
    let mut stmt = StmtUpdate::new();
    assert!(stmt.get_table_reference().is_empty());
    assert!(stmt.get_where_predicate().is_empty());
    assert!(stmt.get_set_values().is_empty());

    stmt.set_table_reference("accounts".to_string());
    assert_eq!(stmt.get_table_reference(), "accounts");

    let assignment = Assignment::new(
        "balance".to_string(),
        AssignedValue::Expression(ExprType::Value(Arc::new(ExprItem::ItemValue(
            ExprValue::ValueLiteral(ExprLiteral::DatumLiteral(DataTyped::from_i32(100))),
        )))),
    );
    stmt.set_set_values(vec![assignment]);
    assert_eq!(stmt.get_set_values().len(), 1);

    stmt.set_where_predicate(vec![sample_predicate()]);
    assert_eq!(stmt.get_where_predicate().len(), 1);
}

#[test]
fn default_update_statement_is_empty() {
    let stmt = StmtUpdate::default();
    assert!(stmt.get_table_reference().is_empty());
}
