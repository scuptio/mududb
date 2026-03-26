use crate::wasm::proc2::object::Wallets;
use mudu::common::result::RS;
use mudu::common::xid::XID;
use mudu_contract::{sql_params, sql_stmt};
use mudu_macro::mudu_proc;
use mudu_type::datum::{Datum, DatumDyn};
use sys_interface::api::{mudu_close, mudu_command, mudu_get, mudu_open, mudu_put, mudu_query, mudu_range};

/**mudu-proc**/
pub fn proc_sys_call(xid: XID, a: i32, b: i64, c: String) -> RS<(i32, String)> {
    let _affected_rows = mudu_command(xid,
                                      &r#"
CREATE TABLE wallets
(
    user_id    INT PRIMARY KEY,
    balance    INT,
    updated_at INT
);"#.to_string(), &vec![])?;

    for i in 1..=2 {
        let _affected_rows = mudu_command(xid,
                                          &r#"
INSERT INTO wallets
(
    user_id,
    balance,
    updated_at
) VALUES (
    ?,
    ?,
    ?
)"#.to_string(), &(i, 100i32, 10000i32))?;
    }

    let wallet_rs = mudu_query::<Wallets>(
        xid,
        sql_stmt!(&"SELECT user_id, balance, updated_at FROM wallets;"),
        sql_params!(&()),
    )?;

    let mut result = String::new();
    while let Some(row) = wallet_rs.next_record()? {
        let value = row.to_value(Wallets::dat_type())?;
        let s = value.to_textual(Wallets::dat_type())?;
        result.push_str(&s);
        result.push('\n');
    };
    Ok(((a + b as i32), format!("xid:{}, a={}, b={}, c={}, result {}", xid, a, b, c, result)))
}


/**mudu-proc**/
pub fn proc2(xid: XID, a: i32, b: i64, c: String) -> RS<(i32, String)> {
    Ok(((a + b as i32), format!("xid:{}, a={}, b={}, c={}", xid, a, b, c)))
}

/**mudu-proc**/
pub fn proc_kv(a: Vec<u8>, b: Vec<u8>) -> RS<usize> {
    let session_id = mudu_open()?;
    mudu_put(session_id, &a, &b)?;
    let value = mudu_get(session_id, &a)?;
    let pairs = mudu_range(session_id, &a, &b)?;
    mudu_close(session_id)?;
    Ok(value.map(|v| v.len()).unwrap_or(0) + pairs.len())
}
