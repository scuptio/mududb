use mududb::common::result::RS;
use mududb::common::xid::XID;

/**mudu-proc**/
pub fn proc_mtp(xid: XID, a: i32, b: i64, c: String) -> RS<(i32, String)> {
    Ok((
        (a + b as i32),
        format!("xid:{}, a={}, b={}, c={}", xid, a, b, c),
    ))
}
