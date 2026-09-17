//! Integration tests for the check-sql driver, including the negative
//! proof fixtures: bad SQL must be flagged with the right location.

#![allow(missing_docs)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::panic)]

use crate::src_check::check_driver::{CheckLang, CheckOutcome, run_check_sql};
use sql_parser::check::diagnostic::SqlSeverity;
use std::path::PathBuf;

fn fixture(name: &str) -> String {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src/src_check/fixtures")
        .join(name)
        .to_string_lossy()
        .to_string()
}

fn schema() -> Vec<String> {
    vec![fixture("check_schema.sql")]
}

fn run(lang: &str, input: &str) -> CheckOutcome {
    run_check_sql(lang, &schema(), &fixture(input)).unwrap()
}

fn rendered_lines(outcome: &CheckOutcome) -> Vec<&str> {
    outcome
        .diagnostics
        .iter()
        .map(|d| d.rendered.as_str())
        .collect()
}

#[test]
fn check_lang_from_name() {
    assert_eq!(CheckLang::from_name("rust"), Some(CheckLang::Rust));
    assert_eq!(CheckLang::from_name("as"), Some(CheckLang::AssemblyScript));
    assert_eq!(
        CheckLang::from_name("assemblyscript"),
        Some(CheckLang::AssemblyScript)
    );
    assert_eq!(CheckLang::from_name("cs"), Some(CheckLang::CSharp));
    assert_eq!(CheckLang::from_name("csharp"), Some(CheckLang::CSharp));
    assert_eq!(CheckLang::from_name("python"), Some(CheckLang::Python));
    assert_eq!(CheckLang::from_name("py"), Some(CheckLang::Python));
    assert_eq!(CheckLang::from_name("c"), Some(CheckLang::C));
    assert_eq!(CheckLang::from_name("cc"), Some(CheckLang::C));
    assert_eq!(CheckLang::from_name("cpp"), Some(CheckLang::C));
    assert_eq!(CheckLang::from_name("go"), Some(CheckLang::Go));
    assert_eq!(CheckLang::from_name("golang"), Some(CheckLang::Go));
    assert_eq!(CheckLang::from_name("cobol"), None);
}

#[test]
fn good_fixture_produces_no_diagnostics() {
    let outcome = run("rust", "good.rs");
    assert_eq!(outcome.file_count, 1);
    assert!(outcome.literal_count >= 4, "got {outcome:?}");
    assert_eq!(outcome.error_count, 0);
    assert_eq!(outcome.uncovered_count, 0);
    assert!(outcome.diagnostics.is_empty(), "got {outcome:?}");
}

#[test]
fn unknown_column_is_flagged_with_location() {
    let outcome = run("rust", "bad_column.rs");
    // The unknown column is reported, and it also breaks the result-shape
    // comparison with the `Wallets` entity.
    assert_eq!(outcome.error_count, 2);
    let lines = rendered_lines(&outcome);
    assert_eq!(lines.len(), 2);
    assert!(
        lines[0].contains("bad_column.rs:5:"),
        "unexpected line: {}",
        lines[0]
    );
    assert!(
        lines[0].contains("error: unknown column 'balances' in table 'wallets'"),
        "unexpected line: {}",
        lines[0]
    );
    assert!(
        lines[1].contains("the SELECT result shape [balances] does not match"),
        "unexpected line: {}",
        lines[1]
    );
    assert_eq!(outcome.diagnostics[0].severity, SqlSeverity::Error);
}

#[test]
fn unknown_table_is_flagged_with_location() {
    let outcome = run("rust", "bad_table.rs");
    assert_eq!(outcome.error_count, 1);
    let lines = rendered_lines(&outcome);
    assert_eq!(lines.len(), 1);
    assert!(
        lines[0].contains("bad_table.rs:5:"),
        "unexpected line: {}",
        lines[0]
    );
    assert!(
        lines[0].contains("error: unknown table 'wallet'"),
        "unexpected line: {}",
        lines[0]
    );
}

#[test]
fn bind_parameter_arity_mismatch_is_flagged() {
    let outcome = run("rust", "bad_arity.rs");
    assert_eq!(outcome.error_count, 1);
    let lines = rendered_lines(&outcome);
    assert!(
        lines[0]
            .contains("the call supplies 1 bind parameter(s) but the SQL has 2 '?' placeholder(s)"),
        "unexpected line: {}",
        lines[0]
    );
}

#[test]
fn select_result_shape_mismatch_is_flagged() {
    let outcome = run("rust", "bad_shape.rs");
    assert_eq!(outcome.error_count, 1);
    let lines = rendered_lines(&outcome);
    assert!(
        lines[0].contains("the SELECT result shape [balance] does not match"),
        "unexpected line: {}",
        lines[0]
    );
    assert!(
        lines[0].contains("'wallets' (Wallets)"),
        "unexpected line: {}",
        lines[0]
    );
}

#[test]
fn join_is_uncovered_not_error() {
    let outcome = run("rust", "uncovered_join.rs");
    assert_eq!(outcome.error_count, 0);
    assert_eq!(outcome.uncovered_count, 1);
    let lines = rendered_lines(&outcome);
    assert!(
        lines[0].contains("uncovered: SQL construct 'join'"),
        "unexpected line: {}",
        lines[0]
    );
    assert_eq!(outcome.diagnostics[0].severity, SqlSeverity::Uncovered);
}

#[test]
fn assemblyscript_unknown_column_is_flagged() {
    let outcome = run("as", "bad_column.ts");
    assert_eq!(outcome.error_count, 1);
    let lines = rendered_lines(&outcome);
    assert!(
        lines[0].contains("bad_column.ts:3:"),
        "unexpected line: {}",
        lines[0]
    );
    assert!(
        lines[0].contains("error: unknown column 'balances'"),
        "unexpected line: {}",
        lines[0]
    );
}

#[test]
fn csharp_errors_cover_arity_and_unknown_table() {
    let outcome = run("cs", "bad_call.cs");
    assert_eq!(outcome.error_count, 2, "got {outcome:?}");
    let lines = rendered_lines(&outcome);
    assert!(
        lines[0]
            .contains("the call supplies 2 bind parameter(s) but the SQL has 1 '?' placeholder(s)"),
        "unexpected line: {}",
        lines[0]
    );
    assert!(
        lines[1].contains("error: unknown table 'wallet'"),
        "unexpected line: {}",
        lines[1]
    );
}

#[test]
fn python_errors_cover_arity_and_unknown_table() {
    let outcome = run("py", "bad_call.py");
    assert_eq!(outcome.error_count, 2, "got {outcome:?}");
    let lines = rendered_lines(&outcome);
    assert!(
        lines[0]
            .contains("the call supplies 2 bind parameter(s) but the SQL has 1 '?' placeholder(s)"),
        "unexpected line: {}",
        lines[0]
    );
    assert!(
        lines[1].contains("error: unknown table 'wallet'"),
        "unexpected line: {}",
        lines[1]
    );
}

#[test]
fn c_errors_cover_arity_and_unknown_table() {
    let outcome = run("c", "bad_call.c");
    assert_eq!(outcome.error_count, 2, "got {outcome:?}");
    let lines = rendered_lines(&outcome);
    assert!(
        lines[0]
            .contains("the call supplies 2 bind parameter(s) but the SQL has 1 '?' placeholder(s)"),
        "unexpected line: {}",
        lines[0]
    );
    assert!(
        lines[1].contains("error: unknown table 'wallet'"),
        "unexpected line: {}",
        lines[1]
    );
}

#[test]
fn go_errors_cover_arity_and_unknown_table() {
    let outcome = run("go", "bad_call.go");
    assert_eq!(outcome.error_count, 2, "got {outcome:?}");
    let lines = rendered_lines(&outcome);
    assert!(
        lines[0]
            .contains("the call supplies 2 bind parameter(s) but the SQL has 1 '?' placeholder(s)"),
        "unexpected line: {}",
        lines[0]
    );
    assert!(
        lines[1].contains("error: unknown table 'wallet'"),
        "unexpected line: {}",
        lines[1]
    );
}

#[test]
fn directory_input_is_scanned_recursively() {
    let outcome = run_check_sql("rust", &schema(), &fixture("good_dir")).unwrap();
    assert_eq!(outcome.file_count, 2);
    assert_eq!(outcome.error_count, 0);
}

#[test]
fn unknown_lang_and_missing_input_are_errors() {
    assert!(run_check_sql("cobol", &schema(), &fixture("good.rs")).is_err());
    assert!(run_check_sql("rust", &schema(), "no/such/path").is_err());
}

#[test]
fn incompatible_literal_param_is_flagged() {
    let outcome = run("rust", "bad_literal_type.rs");
    assert_eq!(outcome.error_count, 1, "got {outcome:?}");
    let lines = rendered_lines(&outcome);
    assert!(
        lines[0].contains(
            "parameter 1: text literal is not compatible with column 'balance' of type I32"
        ),
        "unexpected line: {}",
        lines[0]
    );
}

#[test]
fn compatible_literal_params_produce_no_diagnostics() {
    let outcome = run("rust", "good_literal_type.rs");
    assert_eq!(outcome.error_count, 0, "got {outcome:?}");
    assert_eq!(outcome.uncovered_count, 0, "got {outcome:?}");
}

#[test]
fn named_param_literals_are_checked_in_rewritten_form() {
    let outcome = run("rust", "good_named.rs");
    assert_eq!(outcome.error_count, 0, "got {outcome:?}");
}

#[test]
fn unknown_column_is_caught_after_named_rewrite() {
    let outcome = run("rust", "bad_named_column.rs");
    assert_eq!(outcome.error_count, 1, "got {outcome:?}");
    let lines = rendered_lines(&outcome);
    assert!(
        lines[0].contains("unknown column 'balances'"),
        "unexpected line: {}",
        lines[0]
    );
}
