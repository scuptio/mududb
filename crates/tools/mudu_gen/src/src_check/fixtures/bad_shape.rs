// Negative fixture: a single-column SELECT cannot decode into Wallets.
fn get_balance(xid: u64, user_id: i32) {
    mudu_query::<Wallets>(
        xid,
        sql_stmt!(&"SELECT balance FROM wallets WHERE user_id = ?"),
        sql_params!(&(user_id,)),
    );
}
