//! Unit tests for `StmtSelect`.

#![allow(missing_docs)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::panic)]

use crate::ast::expr_compare::ExprCompare;
use crate::ast::expr_item::{ExprItem, ExprValue};
use crate::ast::expr_literal::ExprLiteral;
use crate::ast::expr_name::ExprName;
use crate::ast::expr_operator::ValueCompare;
use crate::ast::select_term::SelectTerm;
use crate::ast::stmt_select::StmtSelect;
use mudu_type::dat_typed::DatTyped;

fn sample_predicate() -> ExprCompare {
    let mut name = ExprName::new();
    name.set_name("id".to_string());
    let left = ExprItem::ItemName(name);
    let right = ExprItem::ItemValue(ExprValue::ValueLiteral(ExprLiteral::DatumLiteral(
        DatTyped::from_i32(1),
    )));
    ExprCompare::new(ValueCompare::EQ, left, right)
}

#[test]
fn new_creates_empty_select_statement() {
    let stmt = StmtSelect::new();
    assert!(stmt.get_select_term_list().is_empty());
    assert!(stmt.get_table_reference().is_empty());
    assert!(stmt.get_where_predicate().is_empty());
}

#[test]
fn default_creates_empty_select_statement() {
    let stmt = StmtSelect::default();
    assert!(stmt.get_select_term_list().is_empty());
}

#[test]
fn add_select_term_and_predicate() {
    let mut stmt = StmtSelect::new();
    let mut term = SelectTerm::new();
    let mut field = ExprName::new();
    field.set_name("name".to_string());
    term.set_field(field);
    stmt.add_select_term(term);
    assert_eq!(stmt.get_select_term_list().len(), 1);

    stmt.add_where_predicate(sample_predicate());
    assert_eq!(stmt.get_where_predicate().len(), 1);
}

#[test]
fn set_table_reference_updates_from_clause() {
    let mut stmt = StmtSelect::new();
    stmt.set_table_reference("users".to_string());
    assert_eq!(stmt.get_table_reference(), "users");
}
