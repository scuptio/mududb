//! Unit tests for `ExprCompare`.

#![allow(missing_docs)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::panic)]

use crate::ast::expr_compare::ExprCompare;
use crate::ast::expr_item::{ExprItem, ExprValue};
use crate::ast::expr_literal::ExprLiteral;
use crate::ast::expr_name::ExprName;
use crate::ast::expr_operator::ValueCompare;
use mudu_type::data_typed::DataTyped;

fn field(name: &str) -> ExprItem {
    let mut expr = ExprName::new();
    expr.set_name(name.to_string());
    ExprItem::ItemName(expr)
}

fn literal_i32(value: i32) -> ExprItem {
    ExprItem::ItemValue(ExprValue::ValueLiteral(ExprLiteral::DatumLiteral(
        DataTyped::from_i32(value),
    )))
}

#[test]
fn accessors_return_constructor_arguments() {
    let cmp = ExprCompare::new(ValueCompare::EQ, field("id"), literal_i32(7));
    assert!(matches!(cmp.op(), ValueCompare::EQ));
    assert_eq!(cmp.left().to_field().unwrap().name(), "id");
    assert!(cmp.right().to_literal().is_some());
}

#[test]
fn field_op_literal_returns_none_for_field_left_literal_right() {
    // The current implementation only normalizes the reversed order.
    let cmp = ExprCompare::new(ValueCompare::EQ, field("id"), literal_i32(7));
    assert!(cmp.expr_field_op_literal().is_none());
}

#[test]
fn field_op_literal_returns_none_for_non_literal_pairs() {
    let cmp = ExprCompare::new(ValueCompare::GT, field("lhs"), field("rhs"));
    assert!(cmp.expr_field_op_literal().is_none());
}

#[test]
fn field_op_literal_returns_none_for_placeholder() {
    let placeholder = ExprItem::ItemValue(ExprValue::ValuePlaceholder);
    let cmp = ExprCompare::new(ValueCompare::EQ, literal_i32(7), placeholder);
    assert!(cmp.expr_field_op_literal().is_none());
}

#[test]
fn debug_format_includes_operator_and_operands() {
    let cmp = ExprCompare::new(ValueCompare::LT, field("id"), literal_i32(9));
    let debug = format!("{:?}", cmp);
    assert!(debug.contains("op"));
    assert!(debug.contains("left"));
    assert!(debug.contains("right"));
}
