//! Unit tests for the Go SQL-literal extractor.

#![allow(missing_docs)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::panic)]

use crate::src_check::extract_go::extract_go_sql;

#[test]
fn extracts_literals_and_variadic_arity() {
    let source = r#"package main

func createItem(session muduOid, itemID int64, name string) (int64, error) {
	inserted, err := sysCommand(session,
		"INSERT INTO items (item_id, name, quantity) VALUES (?, ?, ?)",
		itemID, name, int64(0))
	rows, err2 := sysQuery(session,
		"SELECT quantity FROM items WHERE item_id = ?",
		itemID)
	_, _, _, _ = inserted, rows, err, err2
	return itemID, nil
}
"#;
    let items = extract_go_sql(source).unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(
        items[0].sql,
        "INSERT INTO items (item_id, name, quantity) VALUES (?, ?, ?)"
    );
    assert_eq!(items[0].param_arity, Some(3));
    assert_eq!((items[0].line, items[0].column), (5, 3));
    assert_eq!(items[1].sql, "SELECT quantity FROM items WHERE item_id = ?");
    assert_eq!(items[1].param_arity, Some(1));
}

#[test]
fn extracts_raw_string_literals() {
    let source = "package main\n\nfunc f(session muduOid, itemID int64) {\n\trows, err := sysQuery(session,\n\t\t`SELECT item_id, name, quantity\n\t\t FROM items WHERE item_id = ?`,\n\t\titemID)\n\t_, _ = rows, err\n}\n";
    let items = extract_go_sql(source).unwrap();
    assert_eq!(items.len(), 1);
    assert!(
        items[0]
            .sql
            .trim_start()
            .starts_with("SELECT item_id, name, quantity")
    );
    assert!(items[0].sql.contains("FROM items WHERE item_id = ?"));
    assert_eq!(items[0].param_arity, Some(1));
}

#[test]
fn decodes_interpreted_escapes() {
    let source =
        "package main\n\nvar sql = \"SELECT \\\"name\\\" FROM items WHERE item_id = ?\\n\"\n";
    let items = extract_go_sql(source).unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(
        items[0].sql,
        "SELECT \"name\" FROM items WHERE item_id = ?\n"
    );
}

#[test]
fn skips_non_sql_and_fragments() {
    let source = "package main\n\nfunc f() {\n\ta := \"item not found\"\n\tb := \"UPDATE \"\n\tc := `plain text`\n\t_, _, _ = a, b, c\n}\n";
    let items = extract_go_sql(source).unwrap();
    assert!(items.is_empty(), "got {items:?}");
}

#[test]
fn spread_and_unknown_callee_have_no_arity() {
    let source = "package main\n\nfunc f(session muduOid, args []any) {\n\ta, _ := sysQuery(session, \"SELECT quantity FROM items WHERE item_id = ?\", args...)\n\tb, _ := helper(session, \"SELECT quantity FROM items WHERE item_id = ?\", 1)\n\t_, _ = a, b\n}\n";
    let items = extract_go_sql(source).unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].param_arity, None);
    assert_eq!(items[1].param_arity, None);
}

#[test]
fn decodes_interpreted_string_escapes() {
    // One literal exercising every escape class: named, octal, hex, short
    // and long unicode code points, and an unknown escape kept verbatim.
    let source = "package main\n\nfunc f() {\n\t_ = \"SELECT * FROM t WHERE c = '\\x41\\101A\\U00000042\\n\\t\\q'\"\n}\n";
    let items = extract_go_sql(source).unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].sql, "SELECT * FROM t WHERE c = 'AAAB\n\t\\q'");
}

#[test]
fn decodes_raw_backtick_strings_verbatim() {
    let source = "package main\n\nfunc f() {\n\t_ = `SELECT * FROM t`\n\t_ = \"SELECT 1\"\n}\n";
    let items = extract_go_sql(source).unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].sql, "SELECT * FROM t");
    assert_eq!(items[1].sql, "SELECT 1");
}

#[test]
fn skips_calls_without_sql_literal_second_argument() {
    // query() whose second argument is a variable (no literal) contributes
    // no arity entry; a call with too few arguments is ignored too.
    let source = r#"package main

func f(s int, q string) {
	_ = query(s, q)
	_ = query(s)
	_ = other(1, "SELECT 1")
}
"#;
    let items = extract_go_sql(source).unwrap();
    // Only the literal under the unrecognized callee is found, with no
    // arity attached.
    assert_eq!(items.len(), 1);
    assert!(items[0].param_arity.is_none());
}

#[test]
fn variadic_spread_call_leaves_arity_unset() {
    let source = r#"package main

func f(s int, args ...any) {
	_ = sysQuery(s, "SELECT * FROM t WHERE id = ?", args...)
}
"#;
    let items = extract_go_sql(source).unwrap();
    assert_eq!(items.len(), 1);
    assert!(items[0].param_arity.is_none());
}

#[test]
fn non_variadic_call_records_trailing_arity() {
    let source = r#"package main

func f(s int, a int, b string) {
	_ = sysCommand(s, "DELETE FROM t WHERE id = ?", a)
}
"#;
    let items = extract_go_sql(source).unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].param_arity, Some(1));
}
