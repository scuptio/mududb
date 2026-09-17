//! Seckill (flash-sale) write-heavy workload procedures.
//!
//! Each `seckill_buy` transaction performs one hot-row UPDATE (stock
//! decrement) plus one order INSERT with a sizable payload, driving the
//! commit/WAL write path instead of read-side machinery. Tables are created
//! client-side (see tpcc_benchmark.rs `init_seckill_schema`), optionally
//! partitioned by `si_id`/`so_item_id` across workers.

use mududb::common::id::OID;
use mududb::common::result::RS;
use mududb::contract::{sql_params, sql_stmt};
use mududb::error::ErrorCode;
use mududb::mudu_error;
use mududb::sys_interface::sync_api::{mudu_command, mudu_query};

fn require_positive(field: &str, value: i32) -> RS<()> {
    if value <= 0 {
        return Err(mudu_error!(
            ErrorCode::InvalidArgument,
            format!("{field} must be positive, got {value}")
        ));
    }
    Ok(())
}

fn query_stock(xid: OID, item_id: i32) -> RS<Option<i32>> {
    let mut rs = mudu_query::<i32>(
        xid,
        sql_stmt!(&"SELECT si_stock FROM seckill_item WHERE si_id = ?"),
        sql_params!(&(item_id,)),
    )?;
    rs.next_record()
}

fn seckill_seed_inner(xid: OID, item_count: i32, initial_stock: i32) -> RS<()> {
    require_positive("item_count", item_count)?;
    require_positive("initial_stock", initial_stock)?;
    for item_id in 1..=item_count {
        mudu_command(
            xid,
            sql_stmt!(
                &"INSERT INTO seckill_item (si_id, si_name, si_stock, si_sold, si_price) VALUES (?, ?, ?, ?, ?)"
            ),
            sql_params!(&(
                item_id,
                format!("promo-item-{item_id}"),
                initial_stock,
                0,
                100
            )),
        )?;
    }
    Ok(())
}

/// Returns "ok" on a successful purchase, "sold_out" when the item has no
/// stock left (the transaction is not aborted; no rows are changed then).
fn seckill_buy_inner(
    xid: OID,
    item_id: i32,
    order_id: i32,
    user_id: i32,
    amount: i32,
    payload: String,
) -> RS<String> {
    require_positive("item_id", item_id)?;
    require_positive("order_id", order_id)?;
    require_positive("user_id", user_id)?;
    require_positive("amount", amount)?;

    let stock = query_stock(xid, item_id)?;
    match stock {
        Some(remaining) if remaining > 0 => {
            mudu_command(
                xid,
                sql_stmt!(
                    &"UPDATE seckill_item SET si_stock = si_stock - 1, si_sold = si_sold + 1 WHERE si_id = ?"
                ),
                sql_params!(&(item_id,)),
            )?;
            mudu_command(
                xid,
                sql_stmt!(
                    &"INSERT INTO seckill_order (so_item_id, so_id, so_user_id, so_amount, so_payload) VALUES (?, ?, ?, ?, ?)"
                ),
                sql_params!(&(item_id, order_id, user_id, amount, payload)),
            )?;
            Ok("ok".to_string())
        }
        _ => Ok("sold_out".to_string()),
    }
}

/**mudu-proc**/
pub fn seckill_seed(xid: OID, item_count: i32, initial_stock: i32) -> RS<()> {
    seckill_seed_inner(xid, item_count, initial_stock)
}

/**mudu-proc**/
pub fn seckill_seed_partitioned(xid: OID, item_count: i32, initial_stock: i32) -> RS<()> {
    seckill_seed_inner(xid, item_count, initial_stock)
}

/**mudu-proc**/
pub fn seckill_buy(
    xid: OID,
    item_id: i32,
    order_id: i32,
    user_id: i32,
    amount: i32,
    payload: String,
) -> RS<String> {
    seckill_buy_inner(xid, item_id, order_id, user_id, amount, payload)
}

/**mudu-proc**/
pub fn seckill_buy_partitioned(
    xid: OID,
    item_id: i32,
    order_id: i32,
    user_id: i32,
    amount: i32,
    payload: String,
) -> RS<String> {
    seckill_buy_inner(xid, item_id, order_id, user_id, amount, payload)
}
