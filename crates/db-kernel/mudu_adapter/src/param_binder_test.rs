//! Unit tests for `param_binder`.

#![allow(missing_docs)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::panic)]

use crate::param_binder::{PlaceholderStyle, normalize_sql};
use mudu::error::ErrorCode;

#[test]
fn question_style_passes_sql_through_after_count_check() {
    let sql = "SELECT a FROM t WHERE x = ? AND y = ?";
    let params = (1_i32, 2_i32);
    let normalized = normalize_sql(sql, &params, PlaceholderStyle::Question).unwrap();
    assert_eq!(normalized, sql);
}

#[test]
fn dollar_number_rewrites_question_marks_in_order() {
    let sql = "INSERT INTO t (a, b, c) VALUES (?, ?, ?)";
    let params = (1_i32, 2_i32, 3_i32);
    let normalized = normalize_sql(sql, &params, PlaceholderStyle::DollarNumber).unwrap();
    assert_eq!(normalized, "INSERT INTO t (a, b, c) VALUES ($1, $2, $3)");
}

#[test]
fn dollar_number_rewrite_keeps_question_marks_in_strings() {
    // The false-positive case the old textual scanner got wrong.
    let sql = "SELECT 'a?b' FROM t WHERE x = ?";
    let params = (1_i32,);
    let normalized = normalize_sql(sql, &params, PlaceholderStyle::DollarNumber).unwrap();
    assert_eq!(normalized, "SELECT 'a?b' FROM t WHERE x = $1");
}

#[test]
fn question_marks_in_comments_do_not_count() {
    let sql = "-- why?\nSELECT a FROM t WHERE x = ?";
    let params = (1_i32,);
    let normalized = normalize_sql(sql, &params, PlaceholderStyle::DollarNumber).unwrap();
    assert_eq!(normalized, "-- why?\nSELECT a FROM t WHERE x = $1");
}

#[test]
fn empty_params_and_no_placeholders_match() {
    let normalized = normalize_sql("SELECT 1", &(), PlaceholderStyle::DollarNumber).unwrap();
    assert_eq!(normalized, "SELECT 1");
}

#[test]
fn count_mismatch_reports_both_counts_and_sql_preview() {
    let sql = "SELECT a FROM t WHERE x = ? AND y = ?";
    let params = (1_i32,);
    let err = normalize_sql(sql, &params, PlaceholderStyle::Question).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::Parse);
    let message = err.message();
    assert!(message.contains("1 parameter(s)"), "{message}");
    assert!(message.contains("2 placeholder(s)"), "{message}");
    assert!(message.contains("SELECT a FROM t"), "{message}");

    let params = (1_i32, 2_i32, 3_i32);
    assert!(normalize_sql(sql, &params, PlaceholderStyle::Question).is_err());
}

#[test]
fn long_sql_preview_is_truncated() {
    let sql = format!(
        "SELECT a FROM t WHERE {} = ? AND {} = ?",
        "x".repeat(100),
        "y".repeat(100)
    );
    let params = (1_i32,);
    let err = normalize_sql(&sql, &params, PlaceholderStyle::Question).unwrap_err();
    assert!(err.message().contains("..."), "{}", err.message());
}

// ---- named (`:name`) parameter resolution ----

use crate::param_binder::resolve_named_sql;
use mudu_contract::database::sql_param_value::SQLParamValue;
use mudu_type::data_value::DataValue;

#[test]
fn named_params_rewrite_and_expand_per_occurrence() {
    let params = SQLParamValue::from_vec_named(
        vec![
            DataValue::from_i32(100),
            DataValue::from_string("x".to_string()),
        ],
        vec!["balance".to_string(), "note".to_string()],
    );
    let (sql, positional) = resolve_named_sql(
        "UPDATE wallets SET balance = :balance, note = :note WHERE balance = :balance",
        &params,
    )
    .unwrap();
    assert_eq!(
        sql,
        "UPDATE wallets SET balance = ?, note = ? WHERE balance = ?"
    );
    let values = positional.params();
    assert_eq!(values.len(), 3);
    // The duplicate :balance reuses its value at both occurrences.
    assert_eq!(values[0].as_i32(), Some(&100));
    assert_eq!(values[1].as_string(), Some(&"x".to_string()));
    assert_eq!(values[2].as_i32(), Some(&100));
}

#[test]
fn named_reorder_follows_occurrence_order_not_declaration_order() {
    let params = SQLParamValue::from_vec_named(
        vec![DataValue::from_i32(1), DataValue::from_i32(2)],
        vec!["first".to_string(), "second".to_string()],
    );
    let (sql, positional) =
        resolve_named_sql("SELECT * FROM t WHERE b = :second AND a = :first", &params).unwrap();
    assert_eq!(sql, "SELECT * FROM t WHERE b = ? AND a = ?");
    let values = positional.params();
    assert_eq!(values[0].as_i32(), Some(&2));
    assert_eq!(values[1].as_i32(), Some(&1));
}

#[test]
fn positional_passthrough_without_names() {
    let params = (1_i32, 2_i32);
    let (sql, positional) =
        resolve_named_sql("SELECT * FROM t WHERE a = ? AND b = ?", &params).unwrap();
    assert_eq!(sql, "SELECT * FROM t WHERE a = ? AND b = ?");
    assert_eq!(positional.params().len(), 2);
}

#[test]
fn named_null_value_flows_through() {
    let params =
        SQLParamValue::from_vec_named(vec![DataValue::null()], vec!["balance".to_string()]);
    let (_sql, positional) =
        resolve_named_sql("UPDATE wallets SET balance = :balance", &params).unwrap();
    assert!(positional.params()[0].is_null());
}

#[test]
fn name_in_sql_missing_from_param_names_is_an_error() {
    let params =
        SQLParamValue::from_vec_named(vec![DataValue::from_i32(1)], vec!["other".to_string()]);
    let err = resolve_named_sql("UPDATE wallets SET balance = :balance", &params).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::Parse);
    assert!(
        err.message()
            .contains("':balance' has no value in param-names"),
        "{}",
        err.message()
    );
}

#[test]
fn unused_param_names_entry_is_an_error() {
    let params = SQLParamValue::from_vec_named(
        vec![DataValue::from_i32(1), DataValue::from_i32(2)],
        vec!["balance".to_string(), "unused".to_string()],
    );
    let err = resolve_named_sql("UPDATE wallets SET balance = :balance", &params).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::Parse);
    assert!(
        err.message()
            .contains("param-names entry 'unused' is not used in the SQL"),
        "{}",
        err.message()
    );
}

#[test]
fn param_names_without_named_placeholders_is_an_error() {
    let params = SQLParamValue::from_vec_named(vec![DataValue::from_i32(1)], vec!["x".to_string()]);
    let err = resolve_named_sql("UPDATE wallets SET balance = ?", &params).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::Parse);
    assert!(
        err.message().contains("the SQL has no named placeholders"),
        "{}",
        err.message()
    );
}

#[test]
fn named_placeholders_without_param_names_is_an_error() {
    let params = (1_i32,);
    let err = resolve_named_sql("UPDATE wallets SET balance = :balance", &params).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::Parse);
    assert!(
        err.message().contains("no parameter names were supplied"),
        "{}",
        err.message()
    );
}

#[test]
fn names_values_count_mismatch_is_an_error() {
    // A mismatched wire frame: two names, one value.
    let uni = mudu_binding::universal::uni_sql_param::UniSqlParam {
        params: vec![
            mudu_binding::universal::uni_data_value::UniDataValue::Scalar(
                mudu_binding::universal::uni_scalar_value::UniScalarValue::from_i32(1),
            ),
        ],
        param_names: Some(vec!["a".to_string(), "b".to_string()]),
    };
    let params = uni.uni_to().unwrap();
    let err = resolve_named_sql("SELECT * FROM t WHERE x = :a AND y = :b", &params).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::Parse);
    assert!(
        err.message().contains("does not match"),
        "{}",
        err.message()
    );
}

#[test]
fn mixing_positional_and_named_is_an_error() {
    let params = SQLParamValue::from_vec_named(vec![DataValue::from_i32(1)], vec!["y".to_string()]);
    let err = resolve_named_sql("SELECT * FROM t WHERE a = ? AND b = :y", &params).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::Parse);
    assert!(
        err.message().contains("mixes positional"),
        "{}",
        err.message()
    );
}
