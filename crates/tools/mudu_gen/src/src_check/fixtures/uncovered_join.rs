// Fixture: JOIN is valid SQL beyond the supported subset -> Uncovered.
fn count_shared(xid: u64, user_id: i32) {
    mudu_query::<i64>(
        xid,
        sql_stmt!(
            &"SELECT COUNT(*) FROM wallets w JOIN users u ON w.user_id = u.user_id WHERE w.user_id = ?"
        ),
        sql_params!(&(user_id,)),
    );
}
