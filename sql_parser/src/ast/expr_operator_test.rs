//! Unit tests for SQL operators.

#![allow(missing_docs)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::panic)]

use crate::ast::expr_operator::{Arithmetic, LogicalConnective, Operator, ValueCompare};
use mudu::error::ErrorCode;

#[test]
fn from_name_parses_comparison_operators() {
    assert!(matches!(
        Operator::from_name("=".to_string()).unwrap(),
        Operator::OValueCompare(ValueCompare::EQ)
    ));
    assert!(matches!(
        Operator::from_name("<".to_string()).unwrap(),
        Operator::OValueCompare(ValueCompare::LT)
    ));
    assert!(matches!(
        Operator::from_name("<=".to_string()).unwrap(),
        Operator::OValueCompare(ValueCompare::LE)
    ));
    assert!(matches!(
        Operator::from_name(">".to_string()).unwrap(),
        Operator::OValueCompare(ValueCompare::GT)
    ));
    assert!(matches!(
        Operator::from_name(">=".to_string()).unwrap(),
        Operator::OValueCompare(ValueCompare::GE)
    ));
    assert!(matches!(
        Operator::from_name("!=".to_string()).unwrap(),
        Operator::OValueCompare(ValueCompare::NE)
    ));
}

#[test]
fn from_name_parses_logical_and_arithmetic_operators() {
    assert!(matches!(
        Operator::from_name("AND".to_string()).unwrap(),
        Operator::OLogicalConnective(LogicalConnective::AND)
    ));
    assert!(matches!(
        Operator::from_name("+".to_string()).unwrap(),
        Operator::OArithmetic(Arithmetic::PLUS)
    ));
    assert!(matches!(
        Operator::from_name("-".to_string()).unwrap(),
        Operator::OArithmetic(Arithmetic::MINUS)
    ));
    assert!(matches!(
        Operator::from_name("*".to_string()).unwrap(),
        Operator::OArithmetic(Arithmetic::MULTIPLE)
    ));
    assert!(matches!(
        Operator::from_name("/".to_string()).unwrap(),
        Operator::OArithmetic(Arithmetic::DIVIDE)
    ));
}

#[test]
fn from_name_rejects_unknown_operator() {
    let result = Operator::from_name("OR".to_string());
    match result {
        Err(err) => {
            assert_eq!(err.ec(), ErrorCode::Parse);
            assert!(err.to_string().contains("OR"));
        }
        Ok(_) => panic!("expected an error for unknown operator"),
    }
}

#[test]
fn logical_connect_returns_logical_variant_only() {
    let and = Operator::from_name("AND".to_string()).unwrap();
    assert!(matches!(
        and.logical_connect(),
        Some(LogicalConnective::AND)
    ));

    let eq = Operator::from_name("=".to_string()).unwrap();
    assert!(eq.logical_connect().is_none());

    let plus = Operator::from_name("+".to_string()).unwrap();
    assert!(plus.logical_connect().is_none());
}

#[test]
fn is_logical_and_identifies_and_only() {
    assert!(Operator::from_name("AND".to_string())
        .unwrap()
        .is_logical_and());
    assert!(!Operator::from_name("=".to_string())
        .unwrap()
        .is_logical_and());
    assert!(!Operator::from_name("+".to_string())
        .unwrap()
        .is_logical_and());
}

#[test]
fn revert_cmp_op_reverses_direction() {
    assert!(matches!(
        ValueCompare::revert_cmp_op(ValueCompare::EQ),
        ValueCompare::EQ
    ));
    assert!(matches!(
        ValueCompare::revert_cmp_op(ValueCompare::LE),
        ValueCompare::GT
    ));
    assert!(matches!(
        ValueCompare::revert_cmp_op(ValueCompare::LT),
        ValueCompare::GE
    ));
    assert!(matches!(
        ValueCompare::revert_cmp_op(ValueCompare::GE),
        ValueCompare::LT
    ));
    assert!(matches!(
        ValueCompare::revert_cmp_op(ValueCompare::GT),
        ValueCompare::LE
    ));
    assert!(matches!(
        ValueCompare::revert_cmp_op(ValueCompare::NE),
        ValueCompare::NE
    ));
}
