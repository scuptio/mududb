use mududb::common::id::OID;
use mududb::common::result::RS;

/**mudu-proc**/
pub fn proc_mtp(xid: OID, a: i32, b: i64, c: String) -> RS<(i32, String)> {
    Ok((
        (a + b as i32),
        format!("xid:{}, a={}, b={}, c={}", xid, a, b, c),
    ))
}
