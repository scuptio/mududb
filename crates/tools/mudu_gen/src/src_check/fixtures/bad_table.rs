// Negative fixture: `wallet` is not a table in the schema.
fn get_balance(xid: u64, user_id: i32) {
    mudu_query::<i32>(
        xid,
        sql_stmt!(&"SELECT balance FROM wallet WHERE user_id = ?"),
        sql_params!(&(user_id,)),
    );
}
