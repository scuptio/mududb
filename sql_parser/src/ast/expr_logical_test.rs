//! Unit tests for `ExprLogical`.

#![allow(missing_docs)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::panic)]

use crate::ast::expr_item::{ExprItem, ExprValue};
use crate::ast::expr_literal::ExprLiteral;
use crate::ast::expr_logical::ExprLogical;
use crate::ast::expr_operator::LogicalConnective;
use crate::ast::expression::ExprType;
use mudu_type::data_typed::DataTyped;
use std::sync::Arc;

fn value_expr(value: i32) -> ExprType {
    ExprType::Value(Arc::new(ExprItem::ItemValue(ExprValue::ValueLiteral(
        ExprLiteral::DatumLiteral(DataTyped::from_i32(value)),
    ))))
}

#[test]
fn logical_expression_stores_operator_and_operands() {
    let left = value_expr(1);
    let right = value_expr(2);
    let logical = ExprLogical::new(LogicalConnective::AND, left.clone(), right.clone());

    assert!(matches!(logical.op(), LogicalConnective::AND));
    assert!(matches!(logical.left(), ExprType::Value(_)));
    assert!(matches!(logical.right(), ExprType::Value(_)));
}

#[test]
fn into_operands_consumes_expression() {
    let left = value_expr(10);
    let right = value_expr(20);
    let logical = ExprLogical::new(LogicalConnective::AND, left, right);

    assert!(matches!(logical.into_left(), ExprType::Value(_)));

    let left = value_expr(10);
    let right = value_expr(20);
    let logical = ExprLogical::new(LogicalConnective::AND, left, right);
    assert!(matches!(logical.into_right(), ExprType::Value(_)));
}
