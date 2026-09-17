//! Unit tests for the SQL checker core.

#![allow(missing_docs)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::panic)]

use crate::ast::parser::SQLParser;
use crate::ast::stmt_select::StmtSelect;
use crate::ast::stmt_type::{StmtCommand, StmtType};
use crate::check::checker::{check_sql, count_params, select_result_shape};
use crate::check::diagnostic::{SqlDiagnostic, SqlSeverity};
use crate::parser::ddl_parser::DDLParser;
use mudu_binding::table::table_def::TableDef;

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

fn errors(diagnostics: &[SqlDiagnostic]) -> Vec<&SqlDiagnostic> {
    diagnostics
        .iter()
        .filter(|d| d.severity() == SqlSeverity::Error)
        .collect()
}

fn uncovered(diagnostics: &[SqlDiagnostic]) -> Vec<&SqlDiagnostic> {
    diagnostics
        .iter()
        .filter(|d| d.severity() == SqlSeverity::Uncovered)
        .collect()
}

fn parse_one(sql: &str) -> StmtType {
    let parser = SQLParser::new().unwrap();
    let stmt_list = parser.parse(sql).unwrap();
    let stmts = stmt_list.stmts();
    assert_eq!(stmts.len(), 1);
    stmts.first().unwrap().clone()
}

// ---- valid statements: no diagnostics ----

#[test]
#[cfg_attr(miri, ignore)]
fn valid_statements_produce_no_diagnostics() {
    let schema = schema();
    for sql in [
        "SELECT balance FROM wallets WHERE user_id = ?",
        "SELECT * FROM wallets WHERE user_id = ?",
        "SELECT user_id, balance, updated_at FROM wallets;",
        "SELECT COUNT(*) FROM wallets WHERE user_id = ?",
        "SELECT COUNT(balance) AS c FROM wallets",
        "SELECT wallets.balance FROM wallets WHERE wallets.user_id = ?",
        "INSERT INTO users (user_id, name, email, created_at, updated_at) VALUES (?, ?, ?, ?, ?)",
        "INSERT INTO wallets (user_id, balance, updated_at) VALUES (?, ?, ?)",
        "UPDATE wallets SET balance = ? WHERE user_id = ?",
        "UPDATE wallets SET balance = balance + 1, updated_at = ? WHERE user_id = ?",
        "DELETE FROM wallets WHERE user_id = ?",
        // case-insensitive identifiers
        "select BALANCE from WALLETS where USER_ID = ?",
        "SELECT balance FROM Wallets WHERE User_Id = ?",
    ] {
        let diagnostics = check_sql(sql, &schema);
        assert!(
            diagnostics.is_empty(),
            "expected no diagnostics for {sql:?}, got {diagnostics:?}"
        );
    }
}

// ---- unknown table / column: Error ----

#[test]
#[cfg_attr(miri, ignore)]
fn unknown_table_is_error() {
    let diagnostics = check_sql("SELECT balance FROM wallet WHERE user_id = ?", &schema());
    let errs = errors(&diagnostics);
    assert_eq!(errs.len(), 1);
    assert!(errs[0].message().contains("unknown table 'wallet'"));
    assert_eq!(errs[0].line(), 1);
    assert_eq!(errs[0].column(), 21);
}

#[test]
#[cfg_attr(miri, ignore)]
fn unknown_column_in_select_list_is_error() {
    let diagnostics = check_sql("SELECT balances FROM wallets WHERE user_id = ?", &schema());
    let errs = errors(&diagnostics);
    assert_eq!(errs.len(), 1);
    assert!(errs[0].message().contains("unknown column 'balances'"));
    assert_eq!(errs[0].column(), 8);
}

#[test]
#[cfg_attr(miri, ignore)]
fn unknown_column_in_where_is_error() {
    let diagnostics = check_sql("SELECT balance FROM wallets WHERE uid = ?", &schema());
    let errs = errors(&diagnostics);
    assert_eq!(errs.len(), 1);
    assert!(errs[0].message().contains("unknown column 'uid'"));
}

#[test]
#[cfg_attr(miri, ignore)]
fn unknown_column_in_insert_and_update_is_error() {
    let diagnostics = check_sql(
        "INSERT INTO wallets (user_id, balances) VALUES (?, ?)",
        &schema(),
    );
    let errs = errors(&diagnostics);
    assert_eq!(errs.len(), 1);
    assert!(errs[0].message().contains("unknown column 'balances'"));

    let diagnostics = check_sql(
        "UPDATE wallets SET balances = ? WHERE user_id = ?",
        &schema(),
    );
    let errs = errors(&diagnostics);
    assert_eq!(errs.len(), 1);
    assert!(errs[0].message().contains("unknown column 'balances'"));

    let diagnostics = check_sql("DELETE FROM accounts WHERE user_id = ?", &schema());
    let errs = errors(&diagnostics);
    assert_eq!(errs.len(), 1);
    assert!(errs[0].message().contains("unknown table 'accounts'"));
}

#[test]
#[cfg_attr(miri, ignore)]
fn qualified_names_check_the_column_part() {
    // The AST converter currently drops the table qualifier of `t.col`
    // references, so only the column part is verified.
    let diagnostics = check_sql(
        "SELECT users.balance FROM wallets WHERE wallets.user_id = ?",
        &schema(),
    );
    assert!(diagnostics.is_empty(), "got {diagnostics:?}");
    let diagnostics = check_sql("SELECT users.balances FROM wallets", &schema());
    let errs = errors(&diagnostics);
    assert_eq!(errs.len(), 1);
    assert!(errs[0].message().contains("unknown column 'balances'"));
}

#[test]
#[cfg_attr(miri, ignore)]
fn case_insensitive_matching_accepts_mixed_case() {
    let diagnostics = check_sql("SELECT Balance FROM WALLETS WHERE USER_ID = ?", &schema());
    assert!(diagnostics.is_empty(), "got {diagnostics:?}");
}

#[test]
#[cfg_attr(miri, ignore)]
fn multiple_diagnostics_are_collected() {
    let diagnostics = check_sql(
        "SELECT nope FROM wallets WHERE missing = ? AND also_missing = ?",
        &schema(),
    );
    let errs = errors(&diagnostics);
    assert_eq!(errs.len(), 3);
}

// ---- syntax errors vs uncovered constructs ----

#[test]
#[cfg_attr(miri, ignore)]
fn garbage_sql_is_error() {
    let diagnostics = check_sql("SELET balance FROM wallets", &schema());
    let errs = errors(&diagnostics);
    assert!(!errs.is_empty());
    assert!(uncovered(&diagnostics).is_empty());
}

#[test]
#[cfg_attr(miri, ignore)]
fn join_is_uncovered_not_error() {
    let sql = "SELECT COUNT(*) FROM vote_choices vc \
               JOIN vote_actions va ON vc.action_id = va.action_id \
               WHERE vc.option_id = ?";
    let diagnostics = check_sql(sql, &schema());
    assert!(errors(&diagnostics).is_empty(), "got {diagnostics:?}");
    assert!(!uncovered(&diagnostics).is_empty());
    assert!(uncovered(&diagnostics)[0].message().contains("join"));
}

#[test]
#[cfg_attr(miri, ignore)]
fn qualified_star_join_is_uncovered_not_error() {
    let sql = "SELECT va.*, v.topic FROM vote_actions va \
               JOIN votes v ON va.vote_id = v.vote_id WHERE user_id = ?";
    let diagnostics = check_sql(sql, &schema());
    assert!(errors(&diagnostics).is_empty(), "got {diagnostics:?}");
    assert!(!uncovered(&diagnostics).is_empty());
}

#[test]
#[cfg_attr(miri, ignore)]
fn multi_argument_function_is_uncovered_not_error() {
    // `COUNT(a, b)` parses at the tree-sitter level but the AST converter
    // rejects it with `NotImplemented`.
    let diagnostics = check_sql("SELECT COUNT(balance, user_id) FROM wallets", &schema());
    assert!(errors(&diagnostics).is_empty(), "got {diagnostics:?}");
    assert!(!uncovered(&diagnostics).is_empty());
}

// ---- parameter counting ----

#[test]
#[cfg_attr(miri, ignore)]
fn count_params_counts_placeholders() {
    let cases: &[(&str, usize)] = &[
        ("SELECT balance FROM wallets WHERE user_id = ?", 1),
        ("SELECT balance FROM wallets", 0),
        (
            "SELECT * FROM wallets WHERE user_id = ? AND balance > ?",
            2,
        ),
        (
            "INSERT INTO users (user_id, name, email, created_at, updated_at) VALUES (?, ?, ?, ?, ?)",
            5,
        ),
        ("INSERT INTO wallets (user_id) VALUES (?), (?)", 2),
        ("UPDATE wallets SET balance = ? WHERE user_id = ?", 2),
        ("UPDATE wallets SET balance = balance + ? WHERE user_id = ?", 2),
        ("DELETE FROM wallets WHERE user_id = ?", 1),
    ];
    for (sql, expected) in cases {
        let stmt = parse_one(sql);
        assert_eq!(count_params(&stmt), *expected, "for {sql:?}");
    }
    let stmt = parse_one("INSERT INTO wallets (user_id, balance) VALUES (?, ?), (?, ?)");
    assert_eq!(count_params(&stmt), 4);
}

#[test]
#[cfg_attr(miri, ignore)]
fn count_params_returns_zero_for_ddl() {
    let stmt = parse_one("CREATE TABLE t (id INT PRIMARY KEY)");
    assert!(matches!(
        stmt,
        StmtType::Command(StmtCommand::CreateTable(_))
    ));
    assert_eq!(count_params(&stmt), 0);
}

// ---- select result shape ----

fn shape_of(sql: &str, schema: &[TableDef]) -> Vec<String> {
    let stmt = parse_one(sql);
    match &stmt {
        StmtType::Select(select) => select_result_shape(select, schema),
        _ => panic!("expected a SELECT statement: {sql:?}"),
    }
}

#[test]
#[cfg_attr(miri, ignore)]
fn select_result_shape_expands_star_in_ddl_order() {
    let schema = schema();
    let shape = shape_of("SELECT * FROM wallets WHERE user_id = ?", &schema);
    assert_eq!(shape, vec!["user_id", "balance", "updated_at"]);
}

#[test]
#[cfg_attr(miri, ignore)]
fn select_result_shape_lists_columns_in_select_order() {
    let schema = schema();
    let shape = shape_of("SELECT balance, user_id FROM wallets", &schema);
    assert_eq!(shape, vec!["balance", "user_id"]);
}

#[test]
#[cfg_attr(miri, ignore)]
fn select_result_shape_marks_functions_and_strips_qualifiers() {
    let schema = schema();
    let shape = shape_of("SELECT COUNT(*) AS c FROM wallets", &schema);
    assert_eq!(shape, vec!["<func>"]);
    let shape = shape_of("SELECT wallets.balance FROM wallets", &schema);
    assert_eq!(shape, vec!["balance"]);
}

#[test]
#[cfg_attr(miri, ignore)]
fn select_result_shape_star_without_schema_degrades_to_star() {
    let select = StmtSelect::new();
    assert_eq!(select_result_shape(&select, &[]), Vec::<String>::new());
    let shape = shape_of("SELECT * FROM nope", &[]);
    assert_eq!(shape, vec!["*"]);
}

// ---- position reporting ----

#[test]
#[cfg_attr(miri, ignore)]
fn positions_are_one_based_and_track_lines() {
    let sql = "SELECT user_id,\n       balances\nFROM wallets";
    let diagnostics = check_sql(sql, &schema());
    let errs = errors(&diagnostics);
    assert_eq!(errs.len(), 1);
    assert_eq!(errs[0].line(), 2);
    assert_eq!(errs[0].column(), 8);
}

#[test]
#[cfg_attr(miri, ignore)]
fn empty_sql_produces_no_diagnostics() {
    assert!(check_sql("", &schema()).is_empty());
    assert!(check_sql("   \n  ", &schema()).is_empty());
}
