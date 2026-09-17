//! Unit tests for the named-parameter scanner.

#![allow(missing_docs)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::panic)]

use crate::check::named_param::rewrite_named_params;
use mudu::error::ErrorCode;

#[test]
fn no_named_params_passes_through() {
    let rewrite = rewrite_named_params("SELECT a FROM t WHERE x = ?").unwrap();
    assert_eq!(rewrite.positional_sql, "SELECT a FROM t WHERE x = ?");
    assert!(rewrite.names_in_order.is_empty());
    assert!(rewrite.occurrences.is_empty());
    assert!(!rewrite.has_named_params());
}

#[test]
fn basic_rewrite_maps_names_in_first_appearance_order() {
    let rewrite = rewrite_named_params("INSERT INTO t (a, b, c) VALUES (:a, :b, :a)").unwrap();
    assert_eq!(
        rewrite.positional_sql,
        "INSERT INTO t (a, b, c) VALUES (?, ?, ?)"
    );
    assert_eq!(rewrite.names_in_order, vec!["a", "b"]);
    // The third `?` reuses the first name's index.
    assert_eq!(rewrite.occurrences, vec![0, 1, 0]);
    assert!(rewrite.has_named_params());
}

#[test]
fn names_with_underscores_and_digits() {
    let rewrite = rewrite_named_params("SELECT * FROM t WHERE user_id = :user_id_2").unwrap();
    assert_eq!(rewrite.names_in_order, vec!["user_id_2"]);
    assert_eq!(rewrite.occurrences, vec![0]);
}

#[test]
fn string_literals_are_skipped() {
    let rewrite = rewrite_named_params("SELECT ':not_a_param' FROM t WHERE x = :x").unwrap();
    assert_eq!(
        rewrite.positional_sql,
        "SELECT ':not_a_param' FROM t WHERE x = ?"
    );
    assert_eq!(rewrite.names_in_order, vec!["x"]);

    let rewrite = rewrite_named_params("SELECT 'it''s :x' FROM t WHERE y = :y").unwrap();
    assert_eq!(
        rewrite.positional_sql,
        "SELECT 'it''s :x' FROM t WHERE y = ?"
    );
    assert_eq!(rewrite.names_in_order, vec!["y"]);

    let rewrite = rewrite_named_params("SELECT \":also_not\" FROM t WHERE z = :z").unwrap();
    assert_eq!(rewrite.names_in_order, vec!["z"]);
}

#[test]
fn comments_are_skipped() {
    let rewrite = rewrite_named_params("-- :not_a_param\nSELECT a FROM t WHERE x = :x").unwrap();
    assert_eq!(
        rewrite.positional_sql,
        "-- :not_a_param\nSELECT a FROM t WHERE x = ?"
    );
    assert_eq!(rewrite.names_in_order, vec!["x"]);

    let rewrite =
        rewrite_named_params("/* :block :comment */ SELECT a FROM t WHERE y = :y").unwrap();
    assert_eq!(
        rewrite.positional_sql,
        "/* :block :comment */ SELECT a FROM t WHERE y = ?"
    );
    assert_eq!(rewrite.names_in_order, vec!["y"]);
}

#[test]
fn postgres_casts_are_not_placeholders() {
    let rewrite = rewrite_named_params("SELECT price::numeric FROM t WHERE x = :x").unwrap();
    assert_eq!(
        rewrite.positional_sql,
        "SELECT price::numeric FROM t WHERE x = ?"
    );
    assert_eq!(rewrite.names_in_order, vec!["x"]);

    let rewrite = rewrite_named_params("SELECT 1::int").unwrap();
    assert!(rewrite.names_in_order.is_empty());
}

#[test]
fn colon_without_identifier_passes_through() {
    let rewrite = rewrite_named_params("SELECT a := 1, b : 2 FROM t").unwrap();
    assert!(rewrite.names_in_order.is_empty());
}

#[test]
fn mixing_question_and_named_is_rejected() {
    let err = rewrite_named_params("SELECT a FROM t WHERE x = ? AND y = :y").unwrap_err();
    assert_eq!(err.ec(), ErrorCode::Parse);
    assert!(
        err.message().contains("mixes positional"),
        "{}",
        err.message()
    );

    let err = rewrite_named_params("SELECT a FROM t WHERE x = $1 AND y = :y").unwrap_err();
    assert_eq!(err.ec(), ErrorCode::Parse);
}
