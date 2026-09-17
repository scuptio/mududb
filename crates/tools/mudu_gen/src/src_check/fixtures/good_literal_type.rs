// Positive fixture: literal parameters that match their target columns,
// plus non-literal elements that must be skipped.
fn create_and_update(xid: u64, next_balance: i32, name: String, email: String) {
    mudu_command(
        xid,
        sql_stmt!(&"INSERT INTO wallets (user_id, balance, updated_at) VALUES (?, ?, ?)"),
        sql_params!(&(1, 100, 0)),
    );
    mudu_command(
        xid,
        sql_stmt!(&"INSERT INTO users (user_id, name, email, created_at, updated_at) VALUES (?, ?, ?, ?, ?)"),
        sql_params!(&(1, "n", "e", 0, 0)),
    );
    mudu_command(
        xid,
        sql_stmt!(&"UPDATE wallets SET balance = ? WHERE user_id = ?"),
        sql_params!(&(next_balance, 1)),
    );
    mudu_command(
        xid,
        sql_stmt!(&"DELETE FROM users WHERE name = ? AND user_id = ?"),
        sql_params!(&("someone", 1)),
    );
}
