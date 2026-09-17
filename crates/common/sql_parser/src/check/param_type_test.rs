//! Unit tests for the placeholder-to-column mapping and the type
//! compatibility table.

#![allow(missing_docs)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::panic)]

use crate::check::param_type::{
    param_target_columns, param_type_compatible, param_type_tag_of_value, uni_data_type_name,
    ParamTypeTag,
};
use crate::parser::ddl_parser::DDLParser;
use mudu_binding::table::table_def::TableDef;
use mudu_type::data_value::DataValue;

const DDL: &str = "
CREATE TABLE users
(
    user_id    INT,
    name       VARCHAR(100),
    email      VARCHAR(100),
    created_at INT,
    updated_at INT,
    PRIMARY KEY (user_id)
);

CREATE TABLE wallets
(
    user_id    INT PRIMARY KEY,
    balance    INT,
    updated_at INT
);
";

fn schema() -> Vec<TableDef> {
    let parser = DDLParser::new().unwrap();
    parser.parse(DDL).unwrap()
}

fn column_names(sql: &str) -> Vec<Option<String>> {
    let schema = schema();
    param_target_columns(sql, &schema)
        .unwrap()
        .into_iter()
        .map(|column| column.map(|c| c.column_name().clone()))
        .collect()
}

#[test]
#[cfg_attr(miri, ignore)]
fn insert_maps_positions_to_column_list() {
    let names = column_names(
        "INSERT INTO users (user_id, name, email, created_at, updated_at) VALUES (?, ?, ?, ?, ?)",
    );
    assert_eq!(
        names,
        vec![
            Some("user_id".to_string()),
            Some("name".to_string()),
            Some("email".to_string()),
            Some("created_at".to_string()),
            Some("updated_at".to_string()),
        ]
    );
}

#[test]
#[cfg_attr(miri, ignore)]
fn insert_skips_literal_positions() {
    let names = column_names("INSERT INTO wallets (user_id, balance, updated_at) VALUES (?, 0, ?)");
    assert_eq!(
        names,
        vec![Some("user_id".to_string()), Some("updated_at".to_string())]
    );
}

#[test]
#[cfg_attr(miri, ignore)]
fn update_maps_assignments_then_where() {
    let names = column_names("UPDATE wallets SET balance = ?, updated_at = ? WHERE user_id = ?");
    assert_eq!(
        names,
        vec![
            Some("balance".to_string()),
            Some("updated_at".to_string()),
            Some("user_id".to_string()),
        ]
    );
}

#[test]
#[cfg_attr(miri, ignore)]
fn update_arithmetic_assignment_maps_to_target_column() {
    let names = column_names("UPDATE wallets SET balance = balance + ? WHERE user_id = ?");
    assert_eq!(
        names,
        vec![Some("balance".to_string()), Some("user_id".to_string())]
    );
}

#[test]
#[cfg_attr(miri, ignore)]
fn where_placeholder_on_either_side_maps_to_the_other_column() {
    let names = column_names("SELECT balance FROM wallets WHERE user_id = ?");
    assert_eq!(names, vec![Some("user_id".to_string())]);
    let names = column_names("SELECT balance FROM wallets WHERE ? < user_id");
    assert_eq!(names, vec![Some("user_id".to_string())]);
}

#[test]
#[cfg_attr(miri, ignore)]
fn delete_where_maps_to_column() {
    let names = column_names("DELETE FROM wallets WHERE user_id = ?");
    assert_eq!(names, vec![Some("user_id".to_string())]);
}

#[test]
#[cfg_attr(miri, ignore)]
fn unmappable_placeholders_are_none_not_error() {
    // `? = ?` has no column context.
    let names = column_names("SELECT balance FROM wallets WHERE ? = ?");
    assert_eq!(names, vec![None, None]);
    // Unknown table.
    let names = column_names("SELECT a FROM nope WHERE x = ?");
    assert_eq!(names, vec![None]);
    // Unknown column.
    let names = column_names("SELECT balance FROM wallets WHERE nope = ?");
    assert_eq!(names, vec![None]);
    // Beyond-subset SQL (JOIN) maps to all-None.
    let names = column_names(
        "SELECT * FROM wallets w JOIN users u ON w.user_id = u.user_id WHERE u.user_id = ?",
    );
    assert_eq!(names, vec![None]);
    // Placeholder in the select list is unsupported and skipped.
    let names = column_names("SELECT ? FROM wallets WHERE user_id = ?");
    assert_eq!(names, vec![None, None]);
}

#[test]
#[cfg_attr(miri, ignore)]
fn no_placeholders_returns_empty() {
    assert!(column_names("SELECT balance FROM wallets").is_empty());
    assert!(column_names("SELET garbage").is_empty());
}

#[test]
fn compatibility_table_rules() {
    let schema = schema();
    let wallets = schema.iter().find(|t| t.table_name() == "wallets").unwrap();
    let int_col = wallets
        .table_columns()
        .iter()
        .find(|c| c.column_name() == "balance")
        .unwrap();
    let users = schema.iter().find(|t| t.table_name() == "users").unwrap();
    let text_col = users
        .table_columns()
        .iter()
        .find(|c| c.column_name() == "name")
        .unwrap();

    // NULL fits everything.
    assert!(param_type_compatible(
        ParamTypeTag::Null,
        int_col.data_type()
    ));
    assert!(param_type_compatible(
        ParamTypeTag::Null,
        text_col.data_type()
    ));
    // Integer fits INT, not TEXT.
    assert!(param_type_compatible(
        ParamTypeTag::Integer,
        int_col.data_type()
    ));
    assert!(!param_type_compatible(
        ParamTypeTag::Integer,
        text_col.data_type()
    ));
    // Text fits VARCHAR, not INT.
    assert!(param_type_compatible(
        ParamTypeTag::Text,
        text_col.data_type()
    ));
    assert!(!param_type_compatible(
        ParamTypeTag::Text,
        int_col.data_type()
    ));
    // Float fits neither INT nor TEXT (only F32/F64/NUMERIC).
    assert!(!param_type_compatible(
        ParamTypeTag::Float,
        int_col.data_type()
    ));
    assert!(!param_type_compatible(
        ParamTypeTag::Float,
        text_col.data_type()
    ));
    // Blob and Boolean fit nothing here.
    assert!(!param_type_compatible(
        ParamTypeTag::Blob,
        text_col.data_type()
    ));
    assert!(!param_type_compatible(
        ParamTypeTag::Boolean,
        int_col.data_type()
    ));
    // Type names for error messages.
    assert_eq!(uni_data_type_name(int_col.data_type()), "I32");
    assert_eq!(uni_data_type_name(text_col.data_type()), "STRING");
}

#[test]
fn value_tagging_covers_families_and_null() {
    assert_eq!(
        param_type_tag_of_value(&DataValue::null()),
        ParamTypeTag::Null
    );
    assert_eq!(
        param_type_tag_of_value(&DataValue::from_i32(1)),
        ParamTypeTag::Integer
    );
    assert_eq!(
        param_type_tag_of_value(&DataValue::from_i64(1)),
        ParamTypeTag::Integer
    );
    assert_eq!(
        param_type_tag_of_value(&DataValue::from_f64(1.0)),
        ParamTypeTag::Float
    );
    assert_eq!(
        param_type_tag_of_value(&DataValue::from_string("s".to_string())),
        ParamTypeTag::Text
    );
    assert_eq!(
        param_type_tag_of_value(&DataValue::from_binary(vec![1])),
        ParamTypeTag::Blob
    );
}
