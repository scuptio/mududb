// Negative fixture: `balances` is not a column of wallets.
fn get_balance(xid: u64, user_id: i32) {
    mudu_query::<Wallets>(
        xid,
        sql_stmt!(&"SELECT balances FROM wallets WHERE user_id = ?"),
        sql_params!(&(user_id,)),
    );
}
