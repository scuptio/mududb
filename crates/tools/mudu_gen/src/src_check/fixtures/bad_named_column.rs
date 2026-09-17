// Negative fixture: an unknown column is still caught when the literal
// uses named placeholders (checked after rewriting to positional form).
fn f(xid: u64, balance: i32) {
    mudu_command(
        xid,
        sql_stmt!(&"UPDATE wallets SET balances = :balance WHERE user_id = :user_id"),
        sql_params!(&(balance, 1)),
    );
}
