//! Unit tests for the C SQL-literal extractor.

#![allow(missing_docs)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::panic)]

use crate::src_check::extract_c::extract_c_sql;

#[test]
fn extracts_literals_and_mudu_call_arity() {
    let source = r#"
static int create_item(const mudu_proc_param *param, mudu_datum *result, mudu_error *err) {
    mudu_datum args[3];
    uint64_t affected = 0;
    mudu_command(param->session,
                 "INSERT INTO items (item_id, name, quantity) VALUES (?, ?, ?)", args, 3,
                 &affected, err);
    mudu_query(param->session, "SELECT quantity FROM items WHERE item_id = ?", args, 1,
               &rows, err);
    return 0;
}
"#;
    let items = extract_c_sql(source).unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(
        items[0].sql,
        "INSERT INTO items (item_id, name, quantity) VALUES (?, ?, ?)"
    );
    assert_eq!(items[0].param_arity, Some(3));
    assert_eq!((items[0].line, items[0].column), (6, 18));
    assert_eq!(items[1].sql, "SELECT quantity FROM items WHERE item_id = ?");
    assert_eq!(items[1].param_arity, Some(1));
}

#[test]
fn joins_adjacent_string_literals() {
    let source = "int f(void) {\n\
        \x20   mudu_query(session,\n\
        \x20                \"SELECT item_id, name, quantity \"\n\
        \x20                \"FROM items WHERE item_id = ?\",\n\
        \x20                args, 1, &rows, err);\n\
        \x20   return 0;\n\
        }\n";
    let items = extract_c_sql(source).unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(
        items[0].sql,
        "SELECT item_id, name, quantity FROM items WHERE item_id = ?"
    );
    assert_eq!(items[0].param_arity, Some(1));
    assert_eq!((items[0].line, items[0].column), (3, 18));
}

#[test]
fn decodes_escape_sequences() {
    let source =
        "const char *sql = \"SELECT \\\"name\\\", \\'q\\' FROM items WHERE item_id = ?\\n\";\n";
    let items = extract_c_sql(source).unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(
        items[0].sql,
        "SELECT \"name\", 'q' FROM items WHERE item_id = ?\n"
    );
}

#[test]
fn skips_non_sql_fragments_and_macro_concatenations() {
    let source = r#"
int f(void) {
    const char *message = "item not found";
    const char *fragment = "UPDATE ";
    printf("deleted %d rows\n", 3);
    printf("delete from " TABLE_NAME "\n");
    return 0;
}
"#;
    let items = extract_c_sql(source).unwrap();
    assert!(items.is_empty(), "got {items:?}");
}

#[test]
fn macro_body_literals_are_checked() {
    let source = "#define CREATE_SQL \"INSERT INTO items (item_id, name, quantity) VALUES (?, ?, ?)\"\n\
                  #define QUERY_SQL \\\n\
                  \x20   \"SELECT item_id, name, quantity \" \\\n\
                  \x20   \"FROM items\"\n\
                  #define MESSAGE \"item not found\"\n";
    let items = extract_c_sql(source).unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(
        items[0].sql,
        "INSERT INTO items (item_id, name, quantity) VALUES (?, ?, ?)"
    );
    assert_eq!((items[0].line, items[0].column), (1, 20));
    assert_eq!(items[1].sql, "SELECT item_id, name, quantity FROM items");
}

#[test]
fn unknown_arity_forms_are_not_recorded() {
    let source = r#"
int f(mudu_oid session, mudu_datum *args, uint32_t n, mudu_rows *rows, mudu_error *err) {
    mudu_query(session, "SELECT quantity FROM items WHERE item_id = ?", args, n, &rows, err);
    helper("SELECT quantity FROM items WHERE item_id = ?", 1);
    return 0;
}
"#;
    let items = extract_c_sql(source).unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].param_arity, None);
    assert_eq!(items[1].param_arity, None);
}
