//! Unit tests for the Rust SQL-literal extractor.

#![allow(missing_docs)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::panic)]

use crate::src_check::extract_rust::extract_rust_sql;
use crate::src_check::sql_literal::{ExtractedSql, dml_keyword, is_sql_candidate};

fn sqls(items: &[ExtractedSql]) -> Vec<&str> {
    items.iter().map(|item| item.sql.as_str()).collect()
}

#[test]
fn dml_keyword_recognizes_dml_and_rejects_lookalikes() {
    assert_eq!(dml_keyword("SELECT a FROM t"), Some("select"));
    assert_eq!(dml_keyword("  insert INTO t VALUES (1)"), Some("insert"));
    assert_eq!(dml_keyword("\n\tUPDATE t SET a = 1"), Some("update"));
    assert_eq!(dml_keyword("Delete FROM t"), Some("delete"));
    // A lone keyword is a fragment, not a statement.
    assert_eq!(dml_keyword("UPDATE "), None);
    assert_eq!(dml_keyword("UPDATE"), None);
    assert_eq!(dml_keyword("SELECT  "), None);
    // Word boundary: longer identifiers are not keywords.
    assert_eq!(dml_keyword("Selects the row"), None);
    assert_eq!(dml_keyword("updated_at = ?"), None);
    assert_eq!(dml_keyword("wallet not found"), None);
    // Natural-language strings are not SQL.
    assert_eq!(dml_keyword("Insert user"), None);
    assert_eq!(dml_keyword("Delete user failed"), None);
    assert_eq!(dml_keyword("Update the balance for the user"), None);
}

#[test]
fn is_sql_candidate_skips_format_templates() {
    assert!(is_sql_candidate("SELECT a FROM t WHERE b = ?"));
    assert!(!is_sql_candidate(
        "SELECT {} FROM {table} WHERE {predicate}"
    ));
    assert!(!is_sql_candidate("INSERT INTO {table} ({}) VALUES ({})"));
    assert!(!is_sql_candidate(""));
    assert!(!is_sql_candidate("user_id = ?"));
}

#[test]
fn extracts_plain_and_macro_wrapped_literals() {
    let source = r#"
const SQL_GET: &str = "SELECT balance FROM wallets WHERE user_id = ?";
fn f(xid: u64) {
    mudu_command(xid, sql_stmt!(&"DELETE FROM wallets WHERE user_id = ?"), sql_params!(&(user_id,)));
}
"#;
    let items = extract_rust_sql(source).unwrap();
    assert_eq!(
        sqls(&items),
        vec![
            "SELECT balance FROM wallets WHERE user_id = ?",
            "DELETE FROM wallets WHERE user_id = ?",
        ]
    );
}

#[test]
fn skips_doc_comments_fragments_templates_and_non_sql() {
    let source = r#"
/// Select one row by primary key.
fn f() {
    let fragment = ["UPDATE ", "wallets", " SET "].concat();
    let dynamic = format!("SELECT {} FROM {table}", "x");
    let message = "Insert user";
    let err = mudu_error!(ErrorCode::DomainViolation, "insufficient funds");
}
"#;
    let items = extract_rust_sql(source).unwrap();
    assert!(items.is_empty(), "got {items:?}");
}

#[test]
fn extracts_raw_strings_with_positions() {
    let source = "fn f(xid: u64) {\n    let _ = mudu_command(\n        xid,\n        sql_stmt!(\n            &r#\"\n        INSERT INTO users (user_id) VALUES (?);\n        \"#\n        ),\n        sql_params!(&(user_id,)),\n    );\n}\n";
    let items = extract_rust_sql(source).unwrap();
    assert_eq!(items.len(), 1);
    assert!(items[0].sql.contains("INSERT INTO users"));
    // The literal token starts at `r` of `r#"` on line 5, column 14.
    assert_eq!((items[0].line, items[0].column), (5, 14));
    assert_eq!(items[0].param_arity, Some(1));
}

#[test]
fn const_indirection_and_dynamic_sql_are_skipped() {
    let source = r#"
fn f(xid: u64, sql: String, params: Vec<Box<dyn DatumDyn>>) {
    mudu_query::<Wallets>(xid, sql_stmt!(&Wallets::SQL_GET_BY_PK), sql_params!(&(user_id,)));
    mudu_command(xid, sql_stmt!(&sql), sql_params!(&params));
    mudu_command(xid, sql_stmt!(&wallets.insert_sql()), sql_params!(&wallets.insert_params()));
}
"#;
    let items = extract_rust_sql(source).unwrap();
    assert!(items.is_empty(), "got {items:?}");
}

#[test]
fn records_param_arity_from_known_param_shapes() {
    let source = r#"
fn f(xid: u64) {
    mudu_command(xid, sql_stmt!(&"UPDATE wallets SET balance = ? WHERE user_id = ?"), sql_params!(&(a, b)));
    mudu_command(xid, sql_stmt!(&"UPDATE wallets SET balance = ? WHERE user_id = ?"), sql_params!(&(a,)));
    mudu_command(xid, sql_stmt!(&"DELETE FROM wallets WHERE user_id = ?"), &(user_id));
    mudu_command(xid, sql_stmt!(&"DELETE FROM wallets"), &vec![]);
    mudu_command(xid, sql_stmt!(&"DELETE FROM wallets WHERE user_id = ?"), sql_params!(&params));
}
"#;
    let items = extract_rust_sql(source).unwrap();
    let arities: Vec<Option<usize>> = items.iter().map(|item| item.param_arity).collect();
    assert_eq!(
        arities,
        vec![Some(2), Some(1), Some(1), Some(0), None],
        "got {items:?}"
    );
}

#[test]
fn records_turbofish_entity() {
    let source = r#"
fn f(xid: u64) {
    let _ = mudu_query::<Wallets>(xid, sql_stmt!(&"SELECT * FROM wallets WHERE user_id = ?"), sql_params!(&(user_id,)));
    let _ = query_one_entity::<Customer>(xid, "SELECT c_id FROM customer WHERE c_id = ?", sql_params!(&(a,)));
    let _ = mudu_query::<i64>(xid, sql_stmt!(&"SELECT COUNT(*) FROM wallets"), sql_params!(&()));
}
"#;
    let items = extract_rust_sql(source).unwrap();
    let entities: Vec<Option<String>> = items.iter().map(|item| item.entity.clone()).collect();
    assert_eq!(
        entities,
        vec![
            Some("Wallets".to_string()),
            Some("Customer".to_string()),
            Some("i64".to_string()),
        ],
        "got {items:?}"
    );
    assert_eq!(items[1].line, 4);
}

#[test]
fn literal_directly_in_call_args_is_extracted() {
    // tpcc style: helpers take the SQL literal as a plain &str argument.
    let source = r#"
fn f(xid: u64) {
    query_one_i32(xid, "SELECT h_counter FROM tpcc_hotspot WHERE h_w_id = ?", sql_params!(&(a, b)));
}
"#;
    let items = extract_rust_sql(source).unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(
        items[0].sql,
        "SELECT h_counter FROM tpcc_hotspot WHERE h_w_id = ?"
    );
    assert_eq!(items[0].param_arity, Some(2));
    assert_eq!(items[0].entity, None);
}

#[test]
fn invalid_rust_returns_error() {
    assert!(extract_rust_sql("fn f( {").is_err());
}

#[test]
fn records_literal_tags_from_param_tuples() {
    use sql_parser::check::param_type::ParamTypeTag;

    let source = r#"
fn f(xid: u64) {
    mudu_command(xid, sql_stmt!(&"INSERT INTO wallets (user_id, balance, updated_at) VALUES (?, ?, ?)"), sql_params!(&(1, "x", 1.5)));
    mudu_command(xid, sql_stmt!(&"DELETE FROM wallets WHERE user_id = ?"), sql_params!(&(user_id,)));
    mudu_command(xid, sql_stmt!(&"UPDATE wallets SET balance = ? WHERE user_id = ?"), &(0, -1));
    mudu_command(xid, sql_stmt!(&"UPDATE wallets SET balance = ? WHERE user_id = ?"), sql_params!(&params));
}
"#;
    let items = extract_rust_sql(source).unwrap();
    let expected: Vec<Vec<Option<ParamTypeTag>>> = vec![
        vec![
            Some(ParamTypeTag::Integer),
            Some(ParamTypeTag::Text),
            Some(ParamTypeTag::Float),
        ],
        vec![None],
        vec![Some(ParamTypeTag::Integer), Some(ParamTypeTag::Integer)],
        // A variable parameter list is not statically typed.
        vec![],
    ];
    let tags: Vec<Vec<Option<ParamTypeTag>>> = items
        .iter()
        .map(|item| item.param_literal_tags.clone())
        .collect();
    assert_eq!(tags, expected, "got {items:?}");
}
