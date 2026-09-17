//! Unit tests for `ExprName`.

#![allow(missing_docs)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::panic)]

use crate::ast::expr_name::ExprName;

#[test]
fn new_creates_empty_name() {
    let name = ExprName::new();
    assert!(name.name().is_empty());
}

#[test]
fn default_creates_empty_name() {
    let name = ExprName::default();
    assert!(name.name().is_empty());
}

#[test]
fn set_name_updates_identifier() {
    let mut name = ExprName::new();
    name.set_name("user_id".to_string());
    assert_eq!(name.name(), "user_id");
}
