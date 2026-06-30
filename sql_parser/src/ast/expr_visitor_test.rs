//! Unit tests for `ExprVisitor`.

#![allow(missing_docs)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::panic)]

use crate::ast::expr_compare::ExprCompare;
use crate::ast::expr_item::{ExprItem, ExprValue};
use crate::ast::expr_literal::ExprLiteral;
use crate::ast::expr_logical::ExprLogical;
use crate::ast::expr_name::ExprName;
use crate::ast::expr_operator::{LogicalConnective, ValueCompare};
use crate::ast::expr_visitor::ExprVisitor;
use crate::ast::expression::ExprType;
use mudu::error::ErrorCode;
use mudu_type::dat_typed::DatTyped;
use std::sync::Arc;

fn compare_expr() -> ExprType {
    let mut name = ExprName::new();
    name.set_name("id".to_string());
    let left = ExprItem::ItemName(name);
    let right = ExprItem::ItemValue(ExprValue::ValueLiteral(ExprLiteral::DatumLiteral(
        DatTyped::from_i32(1),
    )));
    ExprType::Compare(Arc::new(ExprCompare::new(ValueCompare::EQ, left, right)))
}

#[test]
fn extract_from_single_compare() {
    let mut list = Vec::new();
    ExprVisitor::extract_expr_compare_list(compare_expr(), &mut list).unwrap();
    assert_eq!(list.len(), 1);
}

#[test]
fn extract_from_logical_tree() {
    let left = compare_expr();
    let right = compare_expr();
    let logical = ExprType::Logical(Arc::new(ExprLogical::new(
        LogicalConnective::AND,
        left,
        right,
    )));

    let mut list = Vec::new();
    ExprVisitor::extract_expr_compare_list(logical, &mut list).unwrap();
    assert_eq!(list.len(), 2);
}

#[test]
fn extract_rejects_unsupported_expression() {
    let unsupported = ExprType::Value(Arc::new(ExprItem::ItemValue(ExprValue::ValuePlaceholder)));
    let mut list = Vec::new();
    let err = ExprVisitor::extract_expr_compare_list(unsupported, &mut list).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::Parse);
}
