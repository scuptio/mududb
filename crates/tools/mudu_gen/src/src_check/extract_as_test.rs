//! Unit tests for the AssemblyScript SQL-literal extractor.

#![allow(missing_docs)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::panic)]

use crate::src_check::extract_as::extract_as_sql;

#[test]
fn extracts_string_literals_with_positions() {
    let source = r#"function queryBalance(id: Oid, userId: i64): i64 {
  const rows = new ResultSet(witQuery(
    id,
    new SqlStmt("SELECT balance FROM wallets WHERE user_id = ?").raw,
    params.raw,
  ).unwrap());
  return 0;
}
"#;
    let items = extract_as_sql(source).unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(
        items[0].sql,
        "SELECT balance FROM wallets WHERE user_id = ?"
    );
    assert_eq!((items[0].line, items[0].column), (4, 17));
    assert_eq!(items[0].param_arity, None);
}

#[test]
fn extracts_helper_call_literals() {
    let source = r#"const updated = command(
  id,
  "UPDATE wallets SET balance = ? WHERE user_id = ?",
  params,
);
"#;
    let items = extract_as_sql(source).unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(
        items[0].sql,
        "UPDATE wallets SET balance = ? WHERE user_id = ?"
    );
    assert_eq!((items[0].line, items[0].column), (3, 3));
}

#[test]
fn skips_template_strings_and_non_sql() {
    let source = r#"const message = `SELECT ${table} FROM wallets`;
const error = "wallet not found";
const fragment = "UPDATE ";
throw new Error("insufficient funds");
"#;
    let items = extract_as_sql(source).unwrap();
    assert!(items.is_empty(), "got {items:?}");
}
