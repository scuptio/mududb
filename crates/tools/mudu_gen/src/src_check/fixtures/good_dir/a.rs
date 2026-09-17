fn f(xid: u64, user_id: i32) {
    mudu_command(
        xid,
        sql_stmt!(&"DELETE FROM wallets WHERE user_id = ?"),
        sql_params!(&(user_id,)),
    );
}
