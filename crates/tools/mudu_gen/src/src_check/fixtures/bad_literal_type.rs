// Negative fixture: a string literal bound to an INT column.
fn create(xid: u64) {
    mudu_command(
        xid,
        sql_stmt!(&"INSERT INTO wallets (user_id, balance, updated_at) VALUES (?, ?, ?)"),
        sql_params!(&(1, "not-a-number", 0)),
    );
}
