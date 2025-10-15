use mudu::common::result::RS;
use mudu::common::xid::XID;
use mudu::tuple::rs_tuple_datum::RsTupleDatum;
use mudu::{sql_params, sql_stmt};
use mudu_macro::mudu_proc;
use sys_interface::api::mudu_query;

#[mudu_proc]
pub fn proc(xid: XID, id: i32) -> RS<i64> {

    /*
    let result_set = mudu_query::<Order>(
        xid,
        sql_stmt!(&"select order_id, user_id, amount, status from orders where order_id = {};"),
        sql_params!((id))
    )?;
    */
    println!("proc invoked");
    Ok(0)
}


