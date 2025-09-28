use mudu::common::result::RS;
use mudu::common::xid::XID;
use mudu::tuple::rs_tuple_datum::RsTupleDatum;
use mudu::{sql_param, sql_stmt};
use mudu_macro::mudu_proc;
use sys_interface::api::mudu_command;

#[mudu_proc]
pub fn proc(xid: XID, a: i32, b: i64, c: String) -> RS<(i32, String)> {
    /*
    let tuple = (1i32, 0i64);
    let r = mudu_command(
        xid,
        sql_stmt!(&"select a from dual"),
    sql_param!(&tuple))?;
    */
    println!("proc invoked");
    Ok(((a + b as i32), format!("xid:{}, a={}, b={}, c={}", xid, a, b, c)))
}


