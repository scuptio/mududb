// Positive fixture: valid SQL plus all the look-alikes that must be skipped.
/// Select one row by primary key (a doc comment, not SQL).
const SQL_GET_BY_PK: &str = "SELECT user_id, balance, updated_at FROM wallets WHERE user_id = ?";

fn delete_wallet(xid: u64, user_id: i32) {
    // Entity-generated dynamic SQL fragments must be skipped.
    let fragment = ["UPDATE ", "wallets", " SET ", "balance = ?"].concat();
    let _ = fragment;
    // format! templates have holes and must be skipped.
    let dynamic = format!("SELECT {} FROM {table} WHERE {predicate}", "balance");
    let _ = dynamic;
    mudu_command(
        xid,
        sql_stmt!(&"DELETE FROM wallets WHERE user_id = ?"),
        sql_params!(&(user_id,)),
    );
    let _ = SQL_GET_BY_PK;
    let _all = mudu_query::<Wallets>(
        xid,
        sql_stmt!(&"SELECT * FROM wallets WHERE user_id = ?"),
        sql_params!(&(user_id,)),
    );
    let _count = mudu_query::<i64>(xid, sql_stmt!(&"SELECT COUNT(*) FROM wallets"), sql_params!(&()));
    // Raw strings, possibly multi-line, are checked too.
    let _created = mudu_command(
        xid,
        sql_stmt!(
            &r#"
        INSERT INTO users
        (user_id, name, email, created_at, updated_at)
        VALUES (?, ?, ?, ?, ?);
        "#
        ),
        sql_params!(&(user_id, name, email, now, now)),
    );
}
