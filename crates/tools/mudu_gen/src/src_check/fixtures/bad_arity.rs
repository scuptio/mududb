// Negative fixture: the SQL has 2 placeholders but only 1 bind parameter.
fn set_balance(xid: u64, balance: i32) {
    mudu_command(
        xid,
        sql_stmt!(&"UPDATE wallets SET balance = ? WHERE user_id = ?"),
        sql_params!(&(balance,)),
    );
}
