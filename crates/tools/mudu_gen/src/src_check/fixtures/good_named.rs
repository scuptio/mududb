// Positive fixture: named placeholders are rewritten before checking;
// duplicate names and quote/comment look-alikes are all safe.
fn f(xid: u64, balance: i32, user_id: i32) {
    // A named-param literal passed through a local helper (arity is the
    // occurrence count after rewriting — the duplicate :ts counts twice).
    command(
        xid,
        "UPDATE wallets SET balance = :balance, updated_at = :ts WHERE user_id = :uid AND updated_at = :ts",
        sql_params!(&(balance, 0, user_id, 0)),
    );
    // Strings and comments are not placeholders.
    let _ = command(
        xid,
        "SELECT name FROM users WHERE name = ':not_a_param' AND user_id = :uid -- :ignored",
        sql_params!(&(user_id,)),
    );
    // A plain named literal with a matching tuple.
    mudu_command(
        xid,
        sql_stmt!(&"DELETE FROM wallets WHERE user_id = :user_id"),
        sql_params!(&(user_id,)),
    );
}

fn command(xid: u64, sql: &str, params: impl Sized) -> u64 {
    let _ = (xid, sql, params);
    0
}
