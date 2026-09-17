//! Unit tests for the Python SQL-literal extractor.

#![allow(missing_docs)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::panic)]

use crate::src_check::extract_py::extract_py_sql;

#[test]
fn extracts_literals_and_params_chain_arity() {
    let source = r#"from mududb.db import Database
from mududb.sql import Params, SqlStmt


def create_item(session, item_id: int, name: str) -> int:
    db = Database(session)
    inserted = db.command(
        SqlStmt("INSERT INTO items (item_id, name, quantity) VALUES (?, ?, ?)"),
        Params().bind(0, item_id).bind(1, name).bind(2, 0),
    )
    rows = db.query(SqlStmt("SELECT quantity FROM items WHERE item_id = ?"),
                    Params().bind(0, item_id))
    return item_id
"#;
    let items = extract_py_sql(source).unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(
        items[0].sql,
        "INSERT INTO items (item_id, name, quantity) VALUES (?, ?, ?)"
    );
    assert_eq!(items[0].param_arity, Some(3));
    assert_eq!((items[0].line, items[0].column), (8, 17));
    assert_eq!(items[1].sql, "SELECT quantity FROM items WHERE item_id = ?");
    assert_eq!(items[1].param_arity, Some(1));
}

#[test]
fn joins_implicitly_concatenated_strings() {
    let source = "def f(db, item_id):\n\
        \x20   return db.query(SqlStmt(\n\
        \x20       \"SELECT item_id, name, quantity \"\n\
        \x20       \"FROM items WHERE item_id = ?\"),\n\
        \x20       Params().bind(0, item_id))\n";
    let items = extract_py_sql(source).unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(
        items[0].sql,
        "SELECT item_id, name, quantity FROM items WHERE item_id = ?"
    );
    assert_eq!(items[0].param_arity, Some(1));
    assert_eq!((items[0].line, items[0].column), (3, 9));
}

#[test]
fn extracts_triple_quoted_strings() {
    let source = "SQL = \"\"\"\n    SELECT item_id FROM items\n    WHERE name = ?\n\"\"\"\n";
    let items = extract_py_sql(source).unwrap();
    assert_eq!(items.len(), 1);
    assert!(items[0].sql.trim_start().starts_with("SELECT item_id"));
    assert!(items[0].sql.contains("WHERE name = ?"));
    assert_eq!(items[0].param_arity, None);
}

#[test]
fn skips_f_strings_non_sql_and_fragments() {
    let source = r#"def f(db, column):
    a = f"SELECT {column} FROM items"
    b = "item not found"
    c = "UPDATE "
    d = f"SELECT item_id FROM items"
"#;
    let items = extract_py_sql(source).unwrap();
    // Only the hole-free f-string is checked; the interpolated one, the
    // message and the fragment are not SQL candidates.
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].sql, "SELECT item_id FROM items");
}

#[test]
fn named_binds_and_missing_params_have_known_arity() {
    let source = r#"def f(db, name):
    a = db.query(SqlStmt("SELECT item_id FROM items WHERE name = :name"),
                 Params().bind_named("name", name))
    b = db.command(SqlStmt("DELETE FROM items"))
"#;
    let items = extract_py_sql(source).unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].param_arity, Some(1));
    assert_eq!(items[1].param_arity, Some(0));
}

#[test]
fn non_chain_params_and_plain_calls_have_no_arity() {
    let source = r#"def f(db, params):
    a = db.query(SqlStmt("SELECT item_id FROM items WHERE item_id = ?"), params)
    b = run_query("SELECT item_id FROM items")
"#;
    let items = extract_py_sql(source).unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].param_arity, None);
    assert_eq!(items[1].param_arity, None);
}

#[test]
fn keyword_params_argument_is_not_statically_known() {
    let source = "def f(db):\n\
        \x20   db.command(SqlStmt(\"DELETE FROM items WHERE item_id = ?\"), values=make_params())\n";
    let items = extract_py_sql(source).unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].param_arity, None);
}
