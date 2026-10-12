//! The three app1 test procedures, ported from the historical `mudu_wasm`
//! guest crate. Their signatures are pinned by `package/package.desc.json`
//! and by the `mudu_runtime` component/procedure tests.

use crate::rust::wallets::object::Wallets;
use mududb::common::id::OID;
use mududb::common::result::RS;
use mududb::contract::{sql_params, sql_stmt};
use mududb::sys_interface::sync_api::{mudu_command, mudu_query};
use mududb::types::datum::{Datum, DatumDyn};

/**mudu-proc**/
/// Echoes the scalar arguments: returns `(a + b, "xid:.., a=.., b=.., c=..")`.
pub fn proc_mtp(xid: OID, a: i32, b: i64, c: String) -> RS<(i32, String)> {
    Ok((
        (a + b as i32),
        format!("xid:{}, a={}, b={}, c={}", xid, a, b, c),
    ))
}

/**mudu-proc**/
/// Same echo behavior as [`proc_mtp`], declared as a second procedure.
pub fn proc2_mtp(xid: OID, a: i32, b: i64, c: String) -> RS<(i32, String)> {
    Ok((
        (a + b as i32),
        format!("xid:{}, a={}, b={}, c={}", xid, a, b, c),
    ))
}

/**mudu-proc**/
/// Exercises real host syscalls: creates the wallets table, seeds two rows
/// and returns the query results rendered as text.
pub fn proc_sys_call_mtp(xid: OID, a: i32, b: i64, c: String) -> RS<(i32, String)> {
    let _affected_rows = mudu_command(
        xid,
        &r#"
CREATE TABLE wallets
(
    user_id    INT PRIMARY KEY,
    balance    INT,
    updated_at INT
);"#
        .to_string(),
        &vec![],
    )?;

    for i in 1..=2 {
        let _affected_rows = mudu_command(
            xid,
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
)"#
            .to_string(),
            &(i, 100i32, 10000i32),
        )?;
    }

    let wallet_rs = mudu_query::<Wallets>(
        xid,
        sql_stmt!(&"SELECT user_id, balance, updated_at FROM wallets;"),
        sql_params!(&()),
    )?;

    let mut result = String::new();
    while let Some(row) = wallet_rs.next_record()? {
        let value = row.to_value(&Wallets::data_type())?;
        let s = value.to_textual(&Wallets::data_type())?;
        result.push_str(&s);
        result.push('\n');
    }
    Ok((
        (a + b as i32),
        format!("xid:{}, a={}, b={}, c={}, result {}", xid, a, b, c, result),
    ))
}
