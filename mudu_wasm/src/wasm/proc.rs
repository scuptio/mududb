use mudu::common::result::RS;
use mudu::common::xid::XID;
use mudu_macro::mudu_proc;

#[mudu_proc]
pub fn proc(xid: XID, a: i32, b: i64, c: String) -> RS<(i32, String)> {
    Ok(((a + b as i32), format!("xid:{}, a={}, b={}, c={}", xid, a, b, c)))
}




