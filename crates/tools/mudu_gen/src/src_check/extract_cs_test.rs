//! Unit tests for the C# SQL-literal extractor.

#![allow(missing_docs)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::panic)]

use crate::src_check::extract_cs::extract_cs_sql;

#[test]
fn extracts_literals_and_mudusys_arity() {
    let source = r#"internal static class Procedures
{
    public static long QueryBalance(MuduOid session, long userId)
    {
        var rows = MuduSys.Query(session, "SELECT balance FROM wallets WHERE user_id = ?", userId);
        return 0L;
    }

    public static long CreateUser(MuduOid session, long userId, string name, string email)
    {
        return MuduSys.Command(session,
            "INSERT INTO users (user_id, name, email, created_at, updated_at) VALUES (?, ?, ?, ?, ?)",
            userId, name, email, 0L, 0L);
    }
}
"#;
    let items = extract_cs_sql(source).unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(
        items[0].sql,
        "SELECT balance FROM wallets WHERE user_id = ?"
    );
    assert_eq!(items[0].param_arity, Some(1));
    assert_eq!((items[0].line, items[0].column), (5, 43));
    assert!(items[1].sql.starts_with("INSERT INTO users"));
    assert_eq!(items[1].param_arity, Some(5));
    assert_eq!(items[1].line, 12);
}

#[test]
fn skips_interpolated_and_non_sql_strings() {
    let source = r#"class A {
    void M() {
        var message = $"SELECT {column} FROM wallets";
        var error = "wallet not found";
        var fragment = "UPDATE ";
    }
}
"#;
    let items = extract_cs_sql(source).unwrap();
    assert!(items.is_empty(), "got {items:?}");
}

#[test]
fn extracts_verbatim_strings() {
    let source = "class A {\n    void M(MuduOid session, long userId) {\n        var rows = MuduSys.Query(session, @\"SELECT balance FROM wallets WHERE user_id = ?\", userId);\n    }\n}\n";
    let items = extract_cs_sql(source).unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(
        items[0].sql,
        "SELECT balance FROM wallets WHERE user_id = ?"
    );
    assert_eq!(items[0].param_arity, Some(1));
}

#[test]
fn non_mudusys_calls_have_no_arity() {
    let source = r#"class A {
    void M(string sql) {
        Helper.Run("SELECT balance FROM wallets WHERE user_id = ?", 1, 2);
    }
}
"#;
    let items = extract_cs_sql(source).unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].param_arity, None);
}
