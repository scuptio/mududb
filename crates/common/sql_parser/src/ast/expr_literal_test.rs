//! Unit tests for `ExprLiteral`.

#![allow(missing_docs)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::panic)]

use crate::ast::expr_literal::ExprLiteral;
use mudu_type::data_typed::DataTyped;
use mudu_type::type_family::TypeFamily;

#[test]
fn null_literal_has_no_data_type() {
    let literal = ExprLiteral::Null;
    assert!(literal.data_type().is_none());
}

#[test]
fn datum_literal_preserves_underlying_type() {
    let literal = ExprLiteral::DatumLiteral(DataTyped::from_i32(42));
    let typed = literal.data_type().expect("datum literal has a type");
    assert_eq!(typed.data_type().type_family(), TypeFamily::I32);
}
