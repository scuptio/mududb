//! Unit tests for `ExprLiteral`.

#![allow(missing_docs)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::panic)]

use crate::ast::expr_literal::ExprLiteral;
use mudu_type::dat_type_id::DatTypeID;
use mudu_type::dat_typed::DatTyped;

#[test]
fn null_literal_has_no_data_type() {
    let literal = ExprLiteral::Null;
    assert!(literal.dat_type().is_none());
}

#[test]
fn datum_literal_preserves_underlying_type() {
    let literal = ExprLiteral::DatumLiteral(DatTyped::from_i32(42));
    let typed = literal.dat_type().expect("datum literal has a type");
    assert_eq!(typed.dat_type().dat_type_id(), DatTypeID::I32);
}
