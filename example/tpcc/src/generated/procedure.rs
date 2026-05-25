use crate::generated::customer::object::Customer;
use crate::generated::district::object::District;
use crate::generated::item::object::Item;
use crate::generated::new_order::object::NewOrder;
use crate::generated::orders::object::Orders;
use crate::generated::procedure_common::{
    customer_name, district_name, item_name, order_status_text, require_positive,
    validate_order_lines, warehouse_name,
};
use crate::generated::stock::object::Stock;
use crate::generated::warehouse::object::Warehouse;
use mududb::common::result::RS;
use mududb::common::xid::XID;
use mududb::contract::database::entity::Entity;
use mududb::contract::{sql_params, sql_stmt};
use mududb::error::ec::EC::MuduError;
use mududb::m_error;
use mududb::sys_interface::async_api::{mudu_command, mudu_query};

async fn query_one_entity<R: Entity>(
    xid: XID,
    sql: &str,
    params: &dyn mududb::contract::database::sql_params::SQLParams,
) -> RS<R> {
    mudu_query::<R>(xid, sql_stmt!(&sql), params).await?
        .next_record()?
        .ok_or_else(|| m_error!(MuduError, format!("query returned no rows: {sql}")))
}

async fn query_entities<R: Entity>(
    xid: XID,
    sql: &str,
    params: &dyn mududb::contract::database::sql_params::SQLParams,
) -> RS<Vec<R>> {
    let mut result_set = mudu_query::<R>(xid, sql_stmt!(&sql), params).await?;
    let mut values = Vec::new();
    while let Some(value) = result_set.next_record()? {
        values.push(value);
    }
    Ok(values)
}

async fn query_count_i32(
    xid: XID,
    sql: &str,
    params: &dyn mududb::contract::database::sql_params::SQLParams,
) -> RS<i32> {
    let value = mudu_query::<i64>(xid, sql_stmt!(&sql), params).await?
        .next_record()?
        .ok_or_else(|| m_error!(MuduError, format!("query returned no rows: {sql}")))?;
    Ok(value as i32)
}

fn required_i32(value: &Option<i32>, field: &str) -> RS<i32> {
    value
        .as_ref()
        .copied()
        .ok_or_else(|| m_error!(MuduError, format!("entity field is null: {field}")))
}

fn required_string(value: &Option<String>, field: &str) -> RS<String> {
    value
        .clone()
        .ok_or_else(|| m_error!(MuduError, format!("entity field is null: {field}")))
}

async fn tpcc_seed_inner(
    xid: XID,
    warehouse_count: i32,
    district_count: i32,
    customer_count: i32,
    item_count: i32,
    initial_stock: i32,
    warehouse_partitioned: bool,
) -> RS<()> {
    require_positive("warehouse_count", warehouse_count)?;
    require_positive("district_count", district_count)?;
    require_positive("customer_count", customer_count)?;
    require_positive("item_count", item_count)?;
    require_positive("initial_stock", initial_stock)?;

    if warehouse_partitioned {
        for warehouse_id in 1..=warehouse_count {
            for item_id in 1..=item_count {
                mudu_command(
                    xid,
                    sql_stmt!(
                        &"INSERT INTO item (i_w_id, i_id, i_name, i_price) VALUES (?, ?, ?, ?)"
                    ),
                    sql_params!(&(warehouse_id, item_id, item_name(item_id), item_id * 10)),
                ).await?;
            }
        }
    } else {
        for item_id in 1..=item_count {
            mudu_command(
                xid,
                sql_stmt!(&"INSERT INTO item (i_id, i_name, i_price) VALUES (?, ?, ?)"),
                sql_params!(&(item_id, item_name(item_id), item_id * 10)),
            ).await?;
        }
    }
    for warehouse_id in 1..=warehouse_count {
        mudu_command(
            xid,
            sql_stmt!(&"INSERT INTO warehouse (w_id, w_name, w_tax, w_ytd) VALUES (?, ?, ?, 0)"),
            sql_params!(&(warehouse_id, warehouse_name(warehouse_id), warehouse_id % 7)),
        ).await?;
        for district_id in 1..=district_count {
            mudu_command(
                xid,
                sql_stmt!(
                    &"INSERT INTO district (d_id, d_w_id, d_name, d_tax, d_ytd, d_next_o_id, d_last_delivery_o_id) VALUES (?, ?, ?, ?, 0, 1, 0)"
                ),
                sql_params!(&(
                    district_id,
                    warehouse_id,
                    district_name(warehouse_id, district_id),
                    district_id % 9
                )),
            ).await?;
            for customer_id in 1..=customer_count {
                let (first, last) = customer_name(warehouse_id, district_id, customer_id);
                mudu_command(
                    xid,
                    sql_stmt!(
                        &"INSERT INTO customer (c_id, c_d_id, c_w_id, c_first, c_last, c_discount, c_credit, c_balance, c_ytd_payment, c_payment_cnt, c_delivery_cnt, c_last_order_id) VALUES (?, ?, ?, ?, ?, ?, ?, 0, 0, 0, 0, 0)"
                    ),
                    sql_params!(&(
                        customer_id,
                        district_id,
                        warehouse_id,
                        first,
                        last,
                        customer_id % 5,
                        "GC".to_string()
                    )),
                ).await?;
            }
        }
    }
    for warehouse_id in 1..=warehouse_count {
        for item_id in 1..=item_count {
            mudu_command(
                xid,
                sql_stmt!(
                    &"INSERT INTO stock (s_i_id, s_w_id, s_quantity, s_ytd, s_order_cnt, s_remote_cnt) VALUES (?, ?, ?, 0, 0, 0)"
                ),
                sql_params!(&(item_id, warehouse_id, initial_stock)),
            ).await?;
        }
    }
    Ok(())
}

async fn tpcc_new_order_inner(
    xid: XID,
    warehouse_id: i32,
    district_id: i32,
    customer_id: i32,
    item_ids: Vec<i32>,
    supplier_warehouse_ids: Vec<i32>,
    quantities: Vec<i32>,
    warehouse_partitioned: bool,
) -> RS<String> {
    require_positive("warehouse_id", warehouse_id)?;
    require_positive("district_id", district_id)?;
    require_positive("customer_id", customer_id)?;
    validate_order_lines(&item_ids, &supplier_warehouse_ids, &quantities)?;
    if warehouse_partitioned
        && supplier_warehouse_ids
            .iter()
            .any(|&supplier_warehouse_id| supplier_warehouse_id != warehouse_id)
    {
        return Err(m_error!(
            MuduError,
            "partitioned tpcc_new_order requires local supplier warehouses"
        ));
    }

    let district = query_one_entity::<District>(
        xid,
        "SELECT d_id, d_w_id, d_name, d_tax, d_ytd, d_next_o_id, d_last_delivery_o_id FROM district WHERE d_w_id = ? AND d_id = ?",
        sql_params!(&(warehouse_id, district_id)),
    ).await?;
    let next_order_id = required_i32(district.get_d_next_o_id(), "district.d_next_o_id")?;
    let next_d_next_o_id = next_order_id + 1;
    query_one_entity::<Customer>(
        xid,
        "SELECT c_id, c_d_id, c_w_id, c_first, c_last, c_discount, c_credit, c_balance, c_ytd_payment, c_payment_cnt, c_delivery_cnt, c_last_order_id FROM customer WHERE c_w_id = ? AND c_d_id = ? AND c_id = ?",
        sql_params!(&(warehouse_id, district_id, customer_id)),
    ).await?;
    mudu_command(
        xid,
        sql_stmt!(&"UPDATE district SET d_next_o_id = ? WHERE d_w_id = ? AND d_id = ?"),
        sql_params!(&(next_d_next_o_id, warehouse_id, district_id)),
    ).await?;
    let all_local = supplier_warehouse_ids
        .iter()
        .all(|&supplier_warehouse_id| supplier_warehouse_id == warehouse_id);
    let entry_d = format!("xid-{xid}-o{next_order_id}");

    mudu_command(
        xid,
        sql_stmt!(
            &"INSERT INTO orders (o_id, o_d_id, o_w_id, o_c_id, o_entry_d, o_carrier_id, o_ol_cnt, o_all_local, o_status) VALUES (?, ?, ?, ?, ?, 0, ?, ?, ?)"
        ),
        sql_params!(&(
            next_order_id,
            district_id,
            warehouse_id,
            customer_id,
            entry_d,
            item_ids.len() as i32,
            if all_local { 1 } else { 0 },
            "NEW".to_string(),
        )),
    ).await?;
    mudu_command(
        xid,
        sql_stmt!(&"INSERT INTO new_order (no_o_id, no_d_id, no_w_id) VALUES (?, ?, ?)"),
        sql_params!(&(next_order_id, district_id, warehouse_id)),
    ).await?;

    let mut total_quantity = 0;
    let mut total_amount = 0;
    for (idx, ((&item_id, &supplier_warehouse_id), &quantity)) in item_ids
        .iter()
        .zip(supplier_warehouse_ids.iter())
        .zip(quantities.iter())
        .enumerate()
    {
        let item = if warehouse_partitioned {
            query_one_entity::<Item>(
                xid,
                "SELECT i_id, i_name, i_price FROM item WHERE i_w_id = ? AND i_id = ?",
                sql_params!(&(warehouse_id, item_id)),
            ).await?
        } else {
            query_one_entity::<Item>(
                xid,
                "SELECT i_id, i_name, i_price FROM item WHERE i_id = ?",
                sql_params!(&(item_id,)),
            ).await?
        };
        let item_price = required_i32(item.get_i_price(), "item.i_price")?;
        let stock = query_one_entity::<Stock>(
            xid,
            "SELECT s_i_id, s_w_id, s_quantity, s_ytd, s_order_cnt, s_remote_cnt FROM stock WHERE s_w_id = ? AND s_i_id = ?",
            sql_params!(&(supplier_warehouse_id, item_id)),
        ).await?;
        let stock_quantity = required_i32(stock.get_s_quantity(), "stock.s_quantity")?;
        let is_remote = supplier_warehouse_id != warehouse_id;
        let next_stock_ytd = required_i32(stock.get_s_ytd(), "stock.s_ytd")? + quantity;
        let next_stock_order_cnt = required_i32(stock.get_s_order_cnt(), "stock.s_order_cnt")? + 1;
        let next_stock_remote_cnt = required_i32(stock.get_s_remote_cnt(), "stock.s_remote_cnt")?
            + if is_remote { 1 } else { 0 };
        let adjusted_quantity = if stock_quantity >= quantity + 10 {
            stock_quantity - quantity
        } else {
            stock_quantity + 91 - quantity
        };
        let amount = item_price * quantity;

        mudu_command(
            xid,
            sql_stmt!(
                &"UPDATE stock SET s_quantity = ?, s_ytd = ?, s_order_cnt = ?, s_remote_cnt = ? WHERE s_w_id = ? AND s_i_id = ?"
            ),
            sql_params!(&(
                adjusted_quantity,
                next_stock_ytd,
                next_stock_order_cnt,
                next_stock_remote_cnt,
                supplier_warehouse_id,
                item_id
            )),
        ).await?;
        mudu_command(
            xid,
            sql_stmt!(
                &"INSERT INTO order_line (ol_o_id, ol_d_id, ol_w_id, ol_number, ol_i_id, ol_supply_w_id, ol_delivery_d, ol_quantity, ol_amount) VALUES (?, ?, ?, ?, ?, ?, '', ?, ?)"
            ),
            sql_params!(&(
                next_order_id,
                district_id,
                warehouse_id,
                idx as i32 + 1,
                item_id,
                supplier_warehouse_id,
                quantity,
                amount
            )),
        ).await?;
        total_quantity += quantity;
        total_amount += amount;
    }
    mudu_command(
        xid,
        sql_stmt!(
            &"UPDATE customer SET c_last_order_id = ? WHERE c_w_id = ? AND c_d_id = ? AND c_id = ?"
        ),
        sql_params!(&(next_order_id, warehouse_id, district_id, customer_id)),
    ).await?;

    Ok(order_status_text(
        next_order_id,
        item_ids.len(),
        total_quantity,
        total_amount,
        all_local,
        "NEW",
    ))
}

async fn tpcc_payment_inner(
    xid: XID,
    warehouse_id: i32,
    district_id: i32,
    customer_id: i32,
    amount: i32,
    warehouse_partitioned: bool,
) -> RS<i32> {
    require_positive("warehouse_id", warehouse_id)?;
    require_positive("district_id", district_id)?;
    require_positive("customer_id", customer_id)?;
    require_positive("amount", amount)?;

    let warehouse = query_one_entity::<Warehouse>(
        xid,
        "SELECT w_id, w_name, w_tax, w_ytd FROM warehouse WHERE w_id = ?",
        sql_params!(&(warehouse_id,)),
    ).await?;
    let district = query_one_entity::<District>(
        xid,
        "SELECT d_id, d_w_id, d_name, d_tax, d_ytd, d_next_o_id, d_last_delivery_o_id FROM district WHERE d_w_id = ? AND d_id = ?",
        sql_params!(&(warehouse_id, district_id)),
    ).await?;
    let customer = query_one_entity::<Customer>(
        xid,
        "SELECT c_id, c_d_id, c_w_id, c_first, c_last, c_discount, c_credit, c_balance, c_ytd_payment, c_payment_cnt, c_delivery_cnt, c_last_order_id FROM customer WHERE c_w_id = ? AND c_d_id = ? AND c_id = ?",
        sql_params!(&(warehouse_id, district_id, customer_id)),
    ).await?;
    let next_w_ytd = required_i32(warehouse.get_w_ytd(), "warehouse.w_ytd")? + amount;
    let next_d_ytd = required_i32(district.get_d_ytd(), "district.d_ytd")? + amount;
    let next_c_balance = required_i32(customer.get_c_balance(), "customer.c_balance")? - amount;
    let next_c_ytd_payment =
        required_i32(customer.get_c_ytd_payment(), "customer.c_ytd_payment")? + amount;
    let next_c_payment_cnt =
        required_i32(customer.get_c_payment_cnt(), "customer.c_payment_cnt")? + 1;

    mudu_command(
        xid,
        sql_stmt!(&"UPDATE warehouse SET w_ytd = ? WHERE w_id = ?"),
        sql_params!(&(next_w_ytd, warehouse_id)),
    ).await?;
    mudu_command(
        xid,
        sql_stmt!(&"UPDATE district SET d_ytd = ? WHERE d_w_id = ? AND d_id = ?"),
        sql_params!(&(next_d_ytd, warehouse_id, district_id)),
    ).await?;
    mudu_command(
        xid,
        sql_stmt!(
            &"UPDATE customer SET c_balance = ?, c_ytd_payment = ?, c_payment_cnt = ? WHERE c_w_id = ? AND c_d_id = ? AND c_id = ?"
        ),
        sql_params!(&(
            next_c_balance,
            next_c_ytd_payment,
            next_c_payment_cnt,
            warehouse_id,
            district_id,
            customer_id
        )),
    ).await?;
    if warehouse_partitioned {
        mudu_command(
            xid,
            sql_stmt!(
                &"INSERT INTO history (h_w_id, h_id, h_c_id, h_c_d_id, h_c_w_id, h_d_id, h_amount, h_data) VALUES (?, ?, ?, ?, ?, ?, ?, ?)"
            ),
            sql_params!(&(
                warehouse_id,
                mududb::sys::random::next_uuid_v4_string(),
                customer_id,
                district_id,
                warehouse_id,
                district_id,
                amount,
                format!("payment warehouse={warehouse_id} district={district_id}")
            )),
        ).await?;
    } else {
        mudu_command(
            xid,
            sql_stmt!(
                &"INSERT INTO history (h_id, h_c_id, h_c_d_id, h_c_w_id, h_d_id, h_w_id, h_amount, h_data) VALUES (?, ?, ?, ?, ?, ?, ?, ?)"
            ),
            sql_params!(&(
                mududb::sys::random::next_uuid_v4_string(),
                customer_id,
                district_id,
                warehouse_id,
                district_id,
                warehouse_id,
                amount,
                format!("payment warehouse={warehouse_id} district={district_id}")
            )),
        ).await?;
    }
    Ok(next_c_balance)
}

/**mudu-proc**/
pub async fn tpcc_seed(
    xid: XID,
    warehouse_count: i32,
    district_count: i32,
    customer_count: i32,
    item_count: i32,
    initial_stock: i32,
) -> RS<()> {
    tpcc_seed_inner(
        xid,
        warehouse_count,
        district_count,
        customer_count,
        item_count,
        initial_stock,
        false,
    ).await
}

/**mudu-proc**/
pub async fn tpcc_seed_partitioned(
    xid: XID,
    warehouse_count: i32,
    district_count: i32,
    customer_count: i32,
    item_count: i32,
    initial_stock: i32,
) -> RS<()> {
    tpcc_seed_inner(
        xid,
        warehouse_count,
        district_count,
        customer_count,
        item_count,
        initial_stock,
        true,
    ).await
}

/**mudu-proc**/
pub async fn tpcc_new_order(
    xid: XID,
    warehouse_id: i32,
    district_id: i32,
    customer_id: i32,
    item_ids: Vec<i32>,
    supplier_warehouse_ids: Vec<i32>,
    quantities: Vec<i32>,
) -> RS<String> {
    tpcc_new_order_inner(
        xid,
        warehouse_id,
        district_id,
        customer_id,
        item_ids,
        supplier_warehouse_ids,
        quantities,
        false,
    ).await
}

/**mudu-proc**/
pub async fn tpcc_new_order_partitioned(
    xid: XID,
    warehouse_id: i32,
    district_id: i32,
    customer_id: i32,
    item_ids: Vec<i32>,
    supplier_warehouse_ids: Vec<i32>,
    quantities: Vec<i32>,
) -> RS<String> {
    tpcc_new_order_inner(
        xid,
        warehouse_id,
        district_id,
        customer_id,
        item_ids,
        supplier_warehouse_ids,
        quantities,
        true,
    ).await
}

/**mudu-proc**/
pub async fn tpcc_payment(
    xid: XID,
    warehouse_id: i32,
    district_id: i32,
    customer_id: i32,
    amount: i32,
) -> RS<i32> {
    tpcc_payment_inner(xid, warehouse_id, district_id, customer_id, amount, false).await
}

/**mudu-proc**/
pub async fn tpcc_payment_partitioned(
    xid: XID,
    warehouse_id: i32,
    district_id: i32,
    customer_id: i32,
    amount: i32,
) -> RS<i32> {
    tpcc_payment_inner(xid, warehouse_id, district_id, customer_id, amount, true).await
}

/**mudu-proc**/
pub async fn tpcc_order_status(
    xid: XID,
    warehouse_id: i32,
    district_id: i32,
    customer_id: i32,
) -> RS<String> {
    require_positive("warehouse_id", warehouse_id)?;
    require_positive("district_id", district_id)?;
    require_positive("customer_id", customer_id)?;
    let customer = query_one_entity::<Customer>(
        xid,
        "SELECT c_id, c_d_id, c_w_id, c_first, c_last, c_discount, c_credit, c_balance, c_ytd_payment, c_payment_cnt, c_delivery_cnt, c_last_order_id FROM customer WHERE c_w_id = ? AND c_d_id = ? AND c_id = ?",
        sql_params!(&(warehouse_id, district_id, customer_id)),
    ).await?;
    let order_id = required_i32(customer.get_c_last_order_id(), "customer.c_last_order_id")?;
    let order = query_one_entity::<Orders>(
        xid,
        "SELECT o_id, o_d_id, o_w_id, o_c_id, o_entry_d, o_carrier_id, o_ol_cnt, o_all_local, o_status FROM orders WHERE o_w_id = ? AND o_d_id = ? AND o_id = ?",
        sql_params!(&(warehouse_id, district_id, order_id)),
    ).await?;
    required_string(order.get_o_status(), "orders.o_status")
}

/**mudu-proc**/
pub async fn tpcc_order_status_partitioned(
    xid: XID,
    warehouse_id: i32,
    district_id: i32,
    customer_id: i32,
) -> RS<String> {
    tpcc_order_status(xid, warehouse_id, district_id, customer_id).await
}

/**mudu-proc**/
pub async fn tpcc_delivery(xid: XID, warehouse_id: i32, district_id: i32, carrier_id: i32) -> RS<String> {
    require_positive("warehouse_id", warehouse_id)?;
    require_positive("district_id", district_id)?;
    require_positive("carrier_id", carrier_id)?;

    let order_id = query_entities::<NewOrder>(
        xid,
        "SELECT no_o_id, no_d_id, no_w_id FROM new_order WHERE no_w_id = ? AND no_d_id = ?",
        sql_params!(&(warehouse_id, district_id)),
    ).await?
    .into_iter()
    .filter_map(|row| row.get_no_o_id().as_ref().copied())
    .min()
    .ok_or_else(|| m_error!(MuduError, "delivery found no pending new_order rows"))?;
    mudu_command(
        xid,
        sql_stmt!(&"DELETE FROM new_order WHERE no_w_id = ? AND no_d_id = ? AND no_o_id = ?"),
        sql_params!(&(warehouse_id, district_id, order_id)),
    ).await?;
    mudu_command(
        xid,
        sql_stmt!(&"UPDATE district SET d_last_delivery_o_id = ? WHERE d_w_id = ? AND d_id = ?"),
        sql_params!(&(order_id, warehouse_id, district_id)),
    ).await?;
    mudu_command(
        xid,
        sql_stmt!(
            &"UPDATE orders SET o_carrier_id = ?, o_status = ? WHERE o_w_id = ? AND o_d_id = ? AND o_id = ?"
        ),
        sql_params!(&(
            carrier_id,
            "DELIVERED".to_string(),
            warehouse_id,
            district_id,
            order_id
        )),
    ).await?;
    let order = query_one_entity::<Orders>(
        xid,
        "SELECT o_id, o_d_id, o_w_id, o_c_id, o_entry_d, o_carrier_id, o_ol_cnt, o_all_local, o_status FROM orders WHERE o_w_id = ? AND o_d_id = ? AND o_id = ?",
        sql_params!(&(warehouse_id, district_id, order_id)),
    ).await?;
    let customer_id = required_i32(order.get_o_c_id(), "orders.o_c_id")?;
    let customer = query_one_entity::<Customer>(
        xid,
        "SELECT c_id, c_d_id, c_w_id, c_first, c_last, c_discount, c_credit, c_balance, c_ytd_payment, c_payment_cnt, c_delivery_cnt, c_last_order_id FROM customer WHERE c_w_id = ? AND c_d_id = ? AND c_id = ?",
        sql_params!(&(warehouse_id, district_id, customer_id)),
    ).await?;
    let next_delivery_cnt =
        required_i32(customer.get_c_delivery_cnt(), "customer.c_delivery_cnt")? + 1;
    mudu_command(
        xid,
        sql_stmt!(
            &"UPDATE customer SET c_delivery_cnt = ? WHERE c_w_id = ? AND c_d_id = ? AND c_id = ?"
        ),
        sql_params!(&(next_delivery_cnt, warehouse_id, district_id, customer_id)),
    ).await?;
    Ok(format!("delivered order={order_id} carrier={carrier_id}"))
}

/**mudu-proc**/
pub async fn tpcc_delivery_partitioned(
    xid: XID,
    warehouse_id: i32,
    district_id: i32,
    carrier_id: i32,
) -> RS<String> {
    tpcc_delivery(xid, warehouse_id, district_id, carrier_id).await
}

/**mudu-proc**/
pub async fn tpcc_stock_level(xid: XID, warehouse_id: i32, district_id: i32, threshold: i32) -> RS<i32> {
    require_positive("warehouse_id", warehouse_id)?;
    require_positive("district_id", district_id)?;
    require_positive("threshold", threshold)?;
    query_count_i32(
        xid,
        "SELECT COUNT(*) AS field_i64 FROM stock WHERE s_w_id = ? AND s_quantity < ?",
        sql_params!(&(warehouse_id, threshold)),
    ).await
}

/**mudu-proc**/
pub async fn tpcc_stock_level_partitioned(
    xid: XID,
    warehouse_id: i32,
    district_id: i32,
    threshold: i32,
) -> RS<i32> {
    tpcc_stock_level(xid, warehouse_id, district_id, threshold).await
}

#[cfg(test)]
mod tests {
    use super::{
        tpcc_delivery, tpcc_new_order, tpcc_order_status, tpcc_payment, tpcc_seed, tpcc_stock_level,
    };
    use crate::test_lock;
    use mududb::contract::{sql_params, sql_stmt};
    use mududb::sys_interface::async_api::{mudu_batch, mudu_close, mudu_open};
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_db_path(name: &str) -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("tpcc_sql_{name}_{suffix}.db"))
    }

    fn init_schema(xid: u128) {
        let ddl = include_str!("../../sql/ddl.sql");
        let init = include_str!("../../sql/init.sql");
        mudu_batch(xid, sql_stmt!(&ddl), sql_params!(&())).unwrap();
        mudu_batch(xid, sql_stmt!(&init), sql_params!(&())).unwrap();
    }

    #[test]
    async fn tpcc_sync_procedures_roundtrip_against_standalone_adapter() {
        let _guard = test_lock().lock().unwrap_or_else(|err| err.into_inner());
        let db_path = temp_db_path("sync");
        mudu_adapter::config::reset_db_path_override_for_test();
        mudu_adapter::syscall::set_db_path(&db_path);

        let xid = mudu_open().await.unwrap();
        init_schema(xid);
        tpcc_seed(xid, 1, 2, 4, 5, 20).await.unwrap();

        let order =
            tpcc_new_order(xid, 1, 1, 1, vec![2, 4, 5], vec![1, 1, 1], vec![3, 2, 1]).await.unwrap();
        assert!(order.contains("order=1"));
        assert!(order.contains("lines=3"));
        assert!(order.contains("qty=6"));
        assert!(order.contains("amount=190"));
        assert!(order.contains("all_local=true"));
        assert_eq!(tpcc_payment(xid, 1, 1, 1, 7).unwrap(), -7);
        assert_eq!(tpcc_order_status(xid, 1, 1, 1).unwrap(), "NEW");
        assert!(tpcc_delivery(xid, 1, 1, 9).unwrap().contains("carrier=9"));
        assert_eq!(tpcc_order_status(xid, 1, 1, 1).unwrap(), "DELIVERED");
        assert_eq!(tpcc_stock_level(xid, 1, 1, 20).unwrap(), 3);

        mudu_close(xid).await.unwrap();
    }
}
async fn mp2_tpcc_seed_partitioned(param:Vec<u8>) -> Vec<u8> {
    ::mududb::binding::procedure::procedure_invoke::invoke_procedure_async(
        param,
        mudu_inner_p2_tpcc_seed_partitioned,
    ).await
}

pub async fn mudu_inner_p2_tpcc_seed_partitioned(
    param: ::mududb::contract::procedure::procedure_param::ProcedureParam,
) -> ::mududb::common::result::RS<
    ::mududb::contract::procedure::procedure_result::ProcedureResult,
> {
    let res = tpcc_seed_partitioned(
        param.session_id(),
        
            
            ::mududb::types::datum::value_to_typed::<
                i32,
                _,
            >(&param.param_list()[0], "i32")?,
            
        
            
            ::mududb::types::datum::value_to_typed::<
                i32,
                _,
            >(&param.param_list()[1], "i32")?,
            
        
            
            ::mududb::types::datum::value_to_typed::<
                i32,
                _,
            >(&param.param_list()[2], "i32")?,
            
        
            
            ::mududb::types::datum::value_to_typed::<
                i32,
                _,
            >(&param.param_list()[3], "i32")?,
            
        
            
            ::mududb::types::datum::value_to_typed::<
                i32,
                _,
            >(&param.param_list()[4], "i32")?,
            
        
    ).await;
    match res {
        Ok(tuple) => {
            let return_list = {
                
                vec![]
                
            };
            Ok(::mududb::contract::procedure::procedure_result::ProcedureResult::new(return_list))
        }
        Err(e) => Err(e),
    }
}

pub fn mudu_argv_desc_tpcc_seed_partitioned()  -> &'static ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc {
    static ARGV_DESC: std::sync::OnceLock<::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc> =
        std::sync::OnceLock::new();
    ARGV_DESC.get_or_init(||
        {
            ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc::new(vec![
                
                ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                    "warehouse_count".to_string(),
                    
                    <i32 as ::mududb::types::datum::Datum>::dat_type().clone()
                    
                ),
                
                ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                    "district_count".to_string(),
                    
                    <i32 as ::mududb::types::datum::Datum>::dat_type().clone()
                    
                ),
                
                ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                    "customer_count".to_string(),
                    
                    <i32 as ::mududb::types::datum::Datum>::dat_type().clone()
                    
                ),
                
                ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                    "item_count".to_string(),
                    
                    <i32 as ::mududb::types::datum::Datum>::dat_type().clone()
                    
                ),
                
                ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                    "initial_stock".to_string(),
                    
                    <i32 as ::mududb::types::datum::Datum>::dat_type().clone()
                    
                ),
                
            ])
        }
    )
}

pub fn mudu_result_desc_tpcc_seed_partitioned() -> &'static ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc {
    static RESULT_DESC: std::sync::OnceLock<::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc> =
        std::sync::OnceLock::new();
    RESULT_DESC.get_or_init(||
        {
            ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc::new(vec![
                
            ])
        }
    )
}

pub fn mudu_proc_desc_tpcc_seed_partitioned()  -> &'static ::mududb::contract::procedure::proc_desc::ProcDesc {
    static _PROC_DESC: std::sync::OnceLock<
        ::mududb::contract::procedure::proc_desc::ProcDesc,
    > = std::sync::OnceLock::new();
    _PROC_DESC
        .get_or_init(|| {
            ::mududb::contract::procedure::proc_desc::ProcDesc::new(
                "tpcc".to_string(),
                "tpcc_seed_partitioned".to_string(),
                mudu_argv_desc_tpcc_seed_partitioned().clone(),
                mudu_result_desc_tpcc_seed_partitioned().clone(),
                false
            )
        })
}

mod mod_tpcc_seed_partitioned {
    wit_bindgen::generate!({
        inline:
        r##"package mudu:mp2-tpcc-seed-partitioned;
            world mudu-app-mp2-tpcc-seed-partitioned {
                export mp2-tpcc-seed-partitioned: func(param:list<u8>) -> list<u8>;
            }
        "##,
        async: true
    });

    #[allow(non_camel_case_types)]
    #[allow(unused)]
    struct GuestTpccSeedPartitioned {}

    impl Guest for GuestTpccSeedPartitioned {
        async fn mp2_tpcc_seed_partitioned(param:Vec<u8>) -> Vec<u8> {
            super::mp2_tpcc_seed_partitioned(param).await
        }
    }

    export!(GuestTpccSeedPartitioned);
}
async fn mp2_tpcc_payment_partitioned(param:Vec<u8>) -> Vec<u8> {
    ::mududb::binding::procedure::procedure_invoke::invoke_procedure_async(
        param,
        mudu_inner_p2_tpcc_payment_partitioned,
    ).await
}

pub async fn mudu_inner_p2_tpcc_payment_partitioned(
    param: ::mududb::contract::procedure::procedure_param::ProcedureParam,
) -> ::mududb::common::result::RS<
    ::mududb::contract::procedure::procedure_result::ProcedureResult,
> {
    let res = tpcc_payment_partitioned(
        param.session_id(),
        
            
            ::mududb::types::datum::value_to_typed::<
                i32,
                _,
            >(&param.param_list()[0], "i32")?,
            
        
            
            ::mududb::types::datum::value_to_typed::<
                i32,
                _,
            >(&param.param_list()[1], "i32")?,
            
        
            
            ::mududb::types::datum::value_to_typed::<
                i32,
                _,
            >(&param.param_list()[2], "i32")?,
            
        
            
            ::mududb::types::datum::value_to_typed::<
                i32,
                _,
            >(&param.param_list()[3], "i32")?,
            
        
    ).await;
    match res {
        Ok(tuple) => {
            let return_list = {
                
                vec![
                    
                    ::mududb::types::datum::value_from_typed(&tuple, "i32")?
                    
                ]
                
            };
            Ok(::mududb::contract::procedure::procedure_result::ProcedureResult::new(return_list))
        }
        Err(e) => Err(e),
    }
}

pub fn mudu_argv_desc_tpcc_payment_partitioned()  -> &'static ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc {
    static ARGV_DESC: std::sync::OnceLock<::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc> =
        std::sync::OnceLock::new();
    ARGV_DESC.get_or_init(||
        {
            ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc::new(vec![
                
                ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                    "warehouse_id".to_string(),
                    
                    <i32 as ::mududb::types::datum::Datum>::dat_type().clone()
                    
                ),
                
                ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                    "district_id".to_string(),
                    
                    <i32 as ::mududb::types::datum::Datum>::dat_type().clone()
                    
                ),
                
                ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                    "customer_id".to_string(),
                    
                    <i32 as ::mududb::types::datum::Datum>::dat_type().clone()
                    
                ),
                
                ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                    "amount".to_string(),
                    
                    <i32 as ::mududb::types::datum::Datum>::dat_type().clone()
                    
                ),
                
            ])
        }
    )
}

pub fn mudu_result_desc_tpcc_payment_partitioned() -> &'static ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc {
    static RESULT_DESC: std::sync::OnceLock<::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc> =
        std::sync::OnceLock::new();
    RESULT_DESC.get_or_init(||
        {
            ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc::new(vec![
                
                ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                    "0".to_string(),
                    
                    <i32 as ::mududb::types::datum::Datum>::dat_type().clone()
                    
                ),
                
            ])
        }
    )
}

pub fn mudu_proc_desc_tpcc_payment_partitioned()  -> &'static ::mududb::contract::procedure::proc_desc::ProcDesc {
    static _PROC_DESC: std::sync::OnceLock<
        ::mududb::contract::procedure::proc_desc::ProcDesc,
    > = std::sync::OnceLock::new();
    _PROC_DESC
        .get_or_init(|| {
            ::mududb::contract::procedure::proc_desc::ProcDesc::new(
                "tpcc".to_string(),
                "tpcc_payment_partitioned".to_string(),
                mudu_argv_desc_tpcc_payment_partitioned().clone(),
                mudu_result_desc_tpcc_payment_partitioned().clone(),
                false
            )
        })
}

mod mod_tpcc_payment_partitioned {
    wit_bindgen::generate!({
        inline:
        r##"package mudu:mp2-tpcc-payment-partitioned;
            world mudu-app-mp2-tpcc-payment-partitioned {
                export mp2-tpcc-payment-partitioned: func(param:list<u8>) -> list<u8>;
            }
        "##,
        async: true
    });

    #[allow(non_camel_case_types)]
    #[allow(unused)]
    struct GuestTpccPaymentPartitioned {}

    impl Guest for GuestTpccPaymentPartitioned {
        async fn mp2_tpcc_payment_partitioned(param:Vec<u8>) -> Vec<u8> {
            super::mp2_tpcc_payment_partitioned(param).await
        }
    }

    export!(GuestTpccPaymentPartitioned);
}
async fn mp2_tpcc_delivery_partitioned(param:Vec<u8>) -> Vec<u8> {
    ::mududb::binding::procedure::procedure_invoke::invoke_procedure_async(
        param,
        mudu_inner_p2_tpcc_delivery_partitioned,
    ).await
}

pub async fn mudu_inner_p2_tpcc_delivery_partitioned(
    param: ::mududb::contract::procedure::procedure_param::ProcedureParam,
) -> ::mududb::common::result::RS<
    ::mududb::contract::procedure::procedure_result::ProcedureResult,
> {
    let res = tpcc_delivery_partitioned(
        param.session_id(),
        
            
            ::mududb::types::datum::value_to_typed::<
                i32,
                _,
            >(&param.param_list()[0], "i32")?,
            
        
            
            ::mududb::types::datum::value_to_typed::<
                i32,
                _,
            >(&param.param_list()[1], "i32")?,
            
        
            
            ::mududb::types::datum::value_to_typed::<
                i32,
                _,
            >(&param.param_list()[2], "i32")?,
            
        
    ).await;
    match res {
        Ok(tuple) => {
            let return_list = {
                
                vec![
                    
                    ::mududb::types::datum::value_from_typed(&tuple, "String")?
                    
                ]
                
            };
            Ok(::mududb::contract::procedure::procedure_result::ProcedureResult::new(return_list))
        }
        Err(e) => Err(e),
    }
}

pub fn mudu_argv_desc_tpcc_delivery_partitioned()  -> &'static ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc {
    static ARGV_DESC: std::sync::OnceLock<::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc> =
        std::sync::OnceLock::new();
    ARGV_DESC.get_or_init(||
        {
            ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc::new(vec![
                
                ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                    "warehouse_id".to_string(),
                    
                    <i32 as ::mududb::types::datum::Datum>::dat_type().clone()
                    
                ),
                
                ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                    "district_id".to_string(),
                    
                    <i32 as ::mududb::types::datum::Datum>::dat_type().clone()
                    
                ),
                
                ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                    "carrier_id".to_string(),
                    
                    <i32 as ::mududb::types::datum::Datum>::dat_type().clone()
                    
                ),
                
            ])
        }
    )
}

pub fn mudu_result_desc_tpcc_delivery_partitioned() -> &'static ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc {
    static RESULT_DESC: std::sync::OnceLock<::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc> =
        std::sync::OnceLock::new();
    RESULT_DESC.get_or_init(||
        {
            ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc::new(vec![
                
                ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                    "0".to_string(),
                    
                    <String as ::mududb::types::datum::Datum>::dat_type().clone()
                    
                ),
                
            ])
        }
    )
}

pub fn mudu_proc_desc_tpcc_delivery_partitioned()  -> &'static ::mududb::contract::procedure::proc_desc::ProcDesc {
    static _PROC_DESC: std::sync::OnceLock<
        ::mududb::contract::procedure::proc_desc::ProcDesc,
    > = std::sync::OnceLock::new();
    _PROC_DESC
        .get_or_init(|| {
            ::mududb::contract::procedure::proc_desc::ProcDesc::new(
                "tpcc".to_string(),
                "tpcc_delivery_partitioned".to_string(),
                mudu_argv_desc_tpcc_delivery_partitioned().clone(),
                mudu_result_desc_tpcc_delivery_partitioned().clone(),
                false
            )
        })
}

mod mod_tpcc_delivery_partitioned {
    wit_bindgen::generate!({
        inline:
        r##"package mudu:mp2-tpcc-delivery-partitioned;
            world mudu-app-mp2-tpcc-delivery-partitioned {
                export mp2-tpcc-delivery-partitioned: func(param:list<u8>) -> list<u8>;
            }
        "##,
        async: true
    });

    #[allow(non_camel_case_types)]
    #[allow(unused)]
    struct GuestTpccDeliveryPartitioned {}

    impl Guest for GuestTpccDeliveryPartitioned {
        async fn mp2_tpcc_delivery_partitioned(param:Vec<u8>) -> Vec<u8> {
            super::mp2_tpcc_delivery_partitioned(param).await
        }
    }

    export!(GuestTpccDeliveryPartitioned);
}
async fn mp2_tpcc_stock_level_partitioned(param:Vec<u8>) -> Vec<u8> {
    ::mududb::binding::procedure::procedure_invoke::invoke_procedure_async(
        param,
        mudu_inner_p2_tpcc_stock_level_partitioned,
    ).await
}

pub async fn mudu_inner_p2_tpcc_stock_level_partitioned(
    param: ::mududb::contract::procedure::procedure_param::ProcedureParam,
) -> ::mududb::common::result::RS<
    ::mududb::contract::procedure::procedure_result::ProcedureResult,
> {
    let res = tpcc_stock_level_partitioned(
        param.session_id(),
        
            
            ::mududb::types::datum::value_to_typed::<
                i32,
                _,
            >(&param.param_list()[0], "i32")?,
            
        
            
            ::mududb::types::datum::value_to_typed::<
                i32,
                _,
            >(&param.param_list()[1], "i32")?,
            
        
            
            ::mududb::types::datum::value_to_typed::<
                i32,
                _,
            >(&param.param_list()[2], "i32")?,
            
        
    ).await;
    match res {
        Ok(tuple) => {
            let return_list = {
                
                vec![
                    
                    ::mududb::types::datum::value_from_typed(&tuple, "i32")?
                    
                ]
                
            };
            Ok(::mududb::contract::procedure::procedure_result::ProcedureResult::new(return_list))
        }
        Err(e) => Err(e),
    }
}

pub fn mudu_argv_desc_tpcc_stock_level_partitioned()  -> &'static ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc {
    static ARGV_DESC: std::sync::OnceLock<::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc> =
        std::sync::OnceLock::new();
    ARGV_DESC.get_or_init(||
        {
            ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc::new(vec![
                
                ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                    "warehouse_id".to_string(),
                    
                    <i32 as ::mududb::types::datum::Datum>::dat_type().clone()
                    
                ),
                
                ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                    "district_id".to_string(),
                    
                    <i32 as ::mududb::types::datum::Datum>::dat_type().clone()
                    
                ),
                
                ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                    "threshold".to_string(),
                    
                    <i32 as ::mududb::types::datum::Datum>::dat_type().clone()
                    
                ),
                
            ])
        }
    )
}

pub fn mudu_result_desc_tpcc_stock_level_partitioned() -> &'static ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc {
    static RESULT_DESC: std::sync::OnceLock<::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc> =
        std::sync::OnceLock::new();
    RESULT_DESC.get_or_init(||
        {
            ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc::new(vec![
                
                ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                    "0".to_string(),
                    
                    <i32 as ::mududb::types::datum::Datum>::dat_type().clone()
                    
                ),
                
            ])
        }
    )
}

pub fn mudu_proc_desc_tpcc_stock_level_partitioned()  -> &'static ::mududb::contract::procedure::proc_desc::ProcDesc {
    static _PROC_DESC: std::sync::OnceLock<
        ::mududb::contract::procedure::proc_desc::ProcDesc,
    > = std::sync::OnceLock::new();
    _PROC_DESC
        .get_or_init(|| {
            ::mududb::contract::procedure::proc_desc::ProcDesc::new(
                "tpcc".to_string(),
                "tpcc_stock_level_partitioned".to_string(),
                mudu_argv_desc_tpcc_stock_level_partitioned().clone(),
                mudu_result_desc_tpcc_stock_level_partitioned().clone(),
                false
            )
        })
}

mod mod_tpcc_stock_level_partitioned {
    wit_bindgen::generate!({
        inline:
        r##"package mudu:mp2-tpcc-stock-level-partitioned;
            world mudu-app-mp2-tpcc-stock-level-partitioned {
                export mp2-tpcc-stock-level-partitioned: func(param:list<u8>) -> list<u8>;
            }
        "##,
        async: true
    });

    #[allow(non_camel_case_types)]
    #[allow(unused)]
    struct GuestTpccStockLevelPartitioned {}

    impl Guest for GuestTpccStockLevelPartitioned {
        async fn mp2_tpcc_stock_level_partitioned(param:Vec<u8>) -> Vec<u8> {
            super::mp2_tpcc_stock_level_partitioned(param).await
        }
    }

    export!(GuestTpccStockLevelPartitioned);
}
async fn mp2_tpcc_delivery(param:Vec<u8>) -> Vec<u8> {
    ::mududb::binding::procedure::procedure_invoke::invoke_procedure_async(
        param,
        mudu_inner_p2_tpcc_delivery,
    ).await
}

pub async fn mudu_inner_p2_tpcc_delivery(
    param: ::mududb::contract::procedure::procedure_param::ProcedureParam,
) -> ::mududb::common::result::RS<
    ::mududb::contract::procedure::procedure_result::ProcedureResult,
> {
    let res = tpcc_delivery(
        param.session_id(),
        
            
            ::mududb::types::datum::value_to_typed::<
                i32,
                _,
            >(&param.param_list()[0], "i32")?,
            
        
            
            ::mududb::types::datum::value_to_typed::<
                i32,
                _,
            >(&param.param_list()[1], "i32")?,
            
        
            
            ::mududb::types::datum::value_to_typed::<
                i32,
                _,
            >(&param.param_list()[2], "i32")?,
            
        
    ).await;
    match res {
        Ok(tuple) => {
            let return_list = {
                
                vec![
                    
                    ::mududb::types::datum::value_from_typed(&tuple, "String")?
                    
                ]
                
            };
            Ok(::mududb::contract::procedure::procedure_result::ProcedureResult::new(return_list))
        }
        Err(e) => Err(e),
    }
}

pub fn mudu_argv_desc_tpcc_delivery()  -> &'static ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc {
    static ARGV_DESC: std::sync::OnceLock<::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc> =
        std::sync::OnceLock::new();
    ARGV_DESC.get_or_init(||
        {
            ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc::new(vec![
                
                ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                    "warehouse_id".to_string(),
                    
                    <i32 as ::mududb::types::datum::Datum>::dat_type().clone()
                    
                ),
                
                ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                    "district_id".to_string(),
                    
                    <i32 as ::mududb::types::datum::Datum>::dat_type().clone()
                    
                ),
                
                ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                    "carrier_id".to_string(),
                    
                    <i32 as ::mududb::types::datum::Datum>::dat_type().clone()
                    
                ),
                
            ])
        }
    )
}

pub fn mudu_result_desc_tpcc_delivery() -> &'static ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc {
    static RESULT_DESC: std::sync::OnceLock<::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc> =
        std::sync::OnceLock::new();
    RESULT_DESC.get_or_init(||
        {
            ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc::new(vec![
                
                ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                    "0".to_string(),
                    
                    <String as ::mududb::types::datum::Datum>::dat_type().clone()
                    
                ),
                
            ])
        }
    )
}

pub fn mudu_proc_desc_tpcc_delivery()  -> &'static ::mududb::contract::procedure::proc_desc::ProcDesc {
    static _PROC_DESC: std::sync::OnceLock<
        ::mududb::contract::procedure::proc_desc::ProcDesc,
    > = std::sync::OnceLock::new();
    _PROC_DESC
        .get_or_init(|| {
            ::mududb::contract::procedure::proc_desc::ProcDesc::new(
                "tpcc".to_string(),
                "tpcc_delivery".to_string(),
                mudu_argv_desc_tpcc_delivery().clone(),
                mudu_result_desc_tpcc_delivery().clone(),
                false
            )
        })
}

mod mod_tpcc_delivery {
    wit_bindgen::generate!({
        inline:
        r##"package mudu:mp2-tpcc-delivery;
            world mudu-app-mp2-tpcc-delivery {
                export mp2-tpcc-delivery: func(param:list<u8>) -> list<u8>;
            }
        "##,
        async: true
    });

    #[allow(non_camel_case_types)]
    #[allow(unused)]
    struct GuestTpccDelivery {}

    impl Guest for GuestTpccDelivery {
        async fn mp2_tpcc_delivery(param:Vec<u8>) -> Vec<u8> {
            super::mp2_tpcc_delivery(param).await
        }
    }

    export!(GuestTpccDelivery);
}
async fn mp2_tpcc_new_order(param:Vec<u8>) -> Vec<u8> {
    ::mududb::binding::procedure::procedure_invoke::invoke_procedure_async(
        param,
        mudu_inner_p2_tpcc_new_order,
    ).await
}

pub async fn mudu_inner_p2_tpcc_new_order(
    param: ::mududb::contract::procedure::procedure_param::ProcedureParam,
) -> ::mududb::common::result::RS<
    ::mududb::contract::procedure::procedure_result::ProcedureResult,
> {
    let res = tpcc_new_order(
        param.session_id(),
        
            
            ::mududb::types::datum::value_to_typed::<
                i32,
                _,
            >(&param.param_list()[0], "i32")?,
            
        
            
            ::mududb::types::datum::value_to_typed::<
                i32,
                _,
            >(&param.param_list()[1], "i32")?,
            
        
            
            ::mududb::types::datum::value_to_typed::<
                i32,
                _,
            >(&param.param_list()[2], "i32")?,
            
        
            
            ::mududb::types::datum::value_to_typed::<
                Vec<i32, >,
                _,
            >(&param.param_list()[3], "Vec<i32, >")?,
            
        
            
            ::mududb::types::datum::value_to_typed::<
                Vec<i32, >,
                _,
            >(&param.param_list()[4], "Vec<i32, >")?,
            
        
            
            ::mududb::types::datum::value_to_typed::<
                Vec<i32, >,
                _,
            >(&param.param_list()[5], "Vec<i32, >")?,
            
        
    ).await;
    match res {
        Ok(tuple) => {
            let return_list = {
                
                vec![
                    
                    ::mududb::types::datum::value_from_typed(&tuple, "String")?
                    
                ]
                
            };
            Ok(::mududb::contract::procedure::procedure_result::ProcedureResult::new(return_list))
        }
        Err(e) => Err(e),
    }
}

pub fn mudu_argv_desc_tpcc_new_order()  -> &'static ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc {
    static ARGV_DESC: std::sync::OnceLock<::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc> =
        std::sync::OnceLock::new();
    ARGV_DESC.get_or_init(||
        {
            ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc::new(vec![
                
                ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                    "warehouse_id".to_string(),
                    
                    <i32 as ::mududb::types::datum::Datum>::dat_type().clone()
                    
                ),
                
                ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                    "district_id".to_string(),
                    
                    <i32 as ::mududb::types::datum::Datum>::dat_type().clone()
                    
                ),
                
                ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                    "customer_id".to_string(),
                    
                    <i32 as ::mududb::types::datum::Datum>::dat_type().clone()
                    
                ),
                
                ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                    "item_ids".to_string(),
                    
                    <Vec<i32, > as ::mududb::types::datum::Datum>::dat_type().clone()
                    
                ),
                
                ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                    "supplier_warehouse_ids".to_string(),
                    
                    <Vec<i32, > as ::mududb::types::datum::Datum>::dat_type().clone()
                    
                ),
                
                ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                    "quantities".to_string(),
                    
                    <Vec<i32, > as ::mududb::types::datum::Datum>::dat_type().clone()
                    
                ),
                
            ])
        }
    )
}

pub fn mudu_result_desc_tpcc_new_order() -> &'static ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc {
    static RESULT_DESC: std::sync::OnceLock<::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc> =
        std::sync::OnceLock::new();
    RESULT_DESC.get_or_init(||
        {
            ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc::new(vec![
                
                ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                    "0".to_string(),
                    
                    <String as ::mududb::types::datum::Datum>::dat_type().clone()
                    
                ),
                
            ])
        }
    )
}

pub fn mudu_proc_desc_tpcc_new_order()  -> &'static ::mududb::contract::procedure::proc_desc::ProcDesc {
    static _PROC_DESC: std::sync::OnceLock<
        ::mududb::contract::procedure::proc_desc::ProcDesc,
    > = std::sync::OnceLock::new();
    _PROC_DESC
        .get_or_init(|| {
            ::mududb::contract::procedure::proc_desc::ProcDesc::new(
                "tpcc".to_string(),
                "tpcc_new_order".to_string(),
                mudu_argv_desc_tpcc_new_order().clone(),
                mudu_result_desc_tpcc_new_order().clone(),
                false
            )
        })
}

mod mod_tpcc_new_order {
    wit_bindgen::generate!({
        inline:
        r##"package mudu:mp2-tpcc-new-order;
            world mudu-app-mp2-tpcc-new-order {
                export mp2-tpcc-new-order: func(param:list<u8>) -> list<u8>;
            }
        "##,
        async: true
    });

    #[allow(non_camel_case_types)]
    #[allow(unused)]
    struct GuestTpccNewOrder {}

    impl Guest for GuestTpccNewOrder {
        async fn mp2_tpcc_new_order(param:Vec<u8>) -> Vec<u8> {
            super::mp2_tpcc_new_order(param).await
        }
    }

    export!(GuestTpccNewOrder);
}
async fn mp2_tpcc_new_order_partitioned(param:Vec<u8>) -> Vec<u8> {
    ::mududb::binding::procedure::procedure_invoke::invoke_procedure_async(
        param,
        mudu_inner_p2_tpcc_new_order_partitioned,
    ).await
}

pub async fn mudu_inner_p2_tpcc_new_order_partitioned(
    param: ::mududb::contract::procedure::procedure_param::ProcedureParam,
) -> ::mududb::common::result::RS<
    ::mududb::contract::procedure::procedure_result::ProcedureResult,
> {
    let res = tpcc_new_order_partitioned(
        param.session_id(),
        
            
            ::mududb::types::datum::value_to_typed::<
                i32,
                _,
            >(&param.param_list()[0], "i32")?,
            
        
            
            ::mududb::types::datum::value_to_typed::<
                i32,
                _,
            >(&param.param_list()[1], "i32")?,
            
        
            
            ::mududb::types::datum::value_to_typed::<
                i32,
                _,
            >(&param.param_list()[2], "i32")?,
            
        
            
            ::mududb::types::datum::value_to_typed::<
                Vec<i32, >,
                _,
            >(&param.param_list()[3], "Vec<i32, >")?,
            
        
            
            ::mududb::types::datum::value_to_typed::<
                Vec<i32, >,
                _,
            >(&param.param_list()[4], "Vec<i32, >")?,
            
        
            
            ::mududb::types::datum::value_to_typed::<
                Vec<i32, >,
                _,
            >(&param.param_list()[5], "Vec<i32, >")?,
            
        
    ).await;
    match res {
        Ok(tuple) => {
            let return_list = {
                
                vec![
                    
                    ::mududb::types::datum::value_from_typed(&tuple, "String")?
                    
                ]
                
            };
            Ok(::mududb::contract::procedure::procedure_result::ProcedureResult::new(return_list))
        }
        Err(e) => Err(e),
    }
}

pub fn mudu_argv_desc_tpcc_new_order_partitioned()  -> &'static ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc {
    static ARGV_DESC: std::sync::OnceLock<::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc> =
        std::sync::OnceLock::new();
    ARGV_DESC.get_or_init(||
        {
            ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc::new(vec![
                
                ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                    "warehouse_id".to_string(),
                    
                    <i32 as ::mududb::types::datum::Datum>::dat_type().clone()
                    
                ),
                
                ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                    "district_id".to_string(),
                    
                    <i32 as ::mududb::types::datum::Datum>::dat_type().clone()
                    
                ),
                
                ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                    "customer_id".to_string(),
                    
                    <i32 as ::mududb::types::datum::Datum>::dat_type().clone()
                    
                ),
                
                ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                    "item_ids".to_string(),
                    
                    <Vec<i32, > as ::mududb::types::datum::Datum>::dat_type().clone()
                    
                ),
                
                ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                    "supplier_warehouse_ids".to_string(),
                    
                    <Vec<i32, > as ::mududb::types::datum::Datum>::dat_type().clone()
                    
                ),
                
                ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                    "quantities".to_string(),
                    
                    <Vec<i32, > as ::mududb::types::datum::Datum>::dat_type().clone()
                    
                ),
                
            ])
        }
    )
}

pub fn mudu_result_desc_tpcc_new_order_partitioned() -> &'static ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc {
    static RESULT_DESC: std::sync::OnceLock<::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc> =
        std::sync::OnceLock::new();
    RESULT_DESC.get_or_init(||
        {
            ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc::new(vec![
                
                ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                    "0".to_string(),
                    
                    <String as ::mududb::types::datum::Datum>::dat_type().clone()
                    
                ),
                
            ])
        }
    )
}

pub fn mudu_proc_desc_tpcc_new_order_partitioned()  -> &'static ::mududb::contract::procedure::proc_desc::ProcDesc {
    static _PROC_DESC: std::sync::OnceLock<
        ::mududb::contract::procedure::proc_desc::ProcDesc,
    > = std::sync::OnceLock::new();
    _PROC_DESC
        .get_or_init(|| {
            ::mududb::contract::procedure::proc_desc::ProcDesc::new(
                "tpcc".to_string(),
                "tpcc_new_order_partitioned".to_string(),
                mudu_argv_desc_tpcc_new_order_partitioned().clone(),
                mudu_result_desc_tpcc_new_order_partitioned().clone(),
                false
            )
        })
}

mod mod_tpcc_new_order_partitioned {
    wit_bindgen::generate!({
        inline:
        r##"package mudu:mp2-tpcc-new-order-partitioned;
            world mudu-app-mp2-tpcc-new-order-partitioned {
                export mp2-tpcc-new-order-partitioned: func(param:list<u8>) -> list<u8>;
            }
        "##,
        async: true
    });

    #[allow(non_camel_case_types)]
    #[allow(unused)]
    struct GuestTpccNewOrderPartitioned {}

    impl Guest for GuestTpccNewOrderPartitioned {
        async fn mp2_tpcc_new_order_partitioned(param:Vec<u8>) -> Vec<u8> {
            super::mp2_tpcc_new_order_partitioned(param).await
        }
    }

    export!(GuestTpccNewOrderPartitioned);
}
async fn mp2_tpcc_payment(param:Vec<u8>) -> Vec<u8> {
    ::mududb::binding::procedure::procedure_invoke::invoke_procedure_async(
        param,
        mudu_inner_p2_tpcc_payment,
    ).await
}

pub async fn mudu_inner_p2_tpcc_payment(
    param: ::mududb::contract::procedure::procedure_param::ProcedureParam,
) -> ::mududb::common::result::RS<
    ::mududb::contract::procedure::procedure_result::ProcedureResult,
> {
    let res = tpcc_payment(
        param.session_id(),
        
            
            ::mududb::types::datum::value_to_typed::<
                i32,
                _,
            >(&param.param_list()[0], "i32")?,
            
        
            
            ::mududb::types::datum::value_to_typed::<
                i32,
                _,
            >(&param.param_list()[1], "i32")?,
            
        
            
            ::mududb::types::datum::value_to_typed::<
                i32,
                _,
            >(&param.param_list()[2], "i32")?,
            
        
            
            ::mududb::types::datum::value_to_typed::<
                i32,
                _,
            >(&param.param_list()[3], "i32")?,
            
        
    ).await;
    match res {
        Ok(tuple) => {
            let return_list = {
                
                vec![
                    
                    ::mududb::types::datum::value_from_typed(&tuple, "i32")?
                    
                ]
                
            };
            Ok(::mududb::contract::procedure::procedure_result::ProcedureResult::new(return_list))
        }
        Err(e) => Err(e),
    }
}

pub fn mudu_argv_desc_tpcc_payment()  -> &'static ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc {
    static ARGV_DESC: std::sync::OnceLock<::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc> =
        std::sync::OnceLock::new();
    ARGV_DESC.get_or_init(||
        {
            ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc::new(vec![
                
                ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                    "warehouse_id".to_string(),
                    
                    <i32 as ::mududb::types::datum::Datum>::dat_type().clone()
                    
                ),
                
                ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                    "district_id".to_string(),
                    
                    <i32 as ::mududb::types::datum::Datum>::dat_type().clone()
                    
                ),
                
                ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                    "customer_id".to_string(),
                    
                    <i32 as ::mududb::types::datum::Datum>::dat_type().clone()
                    
                ),
                
                ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                    "amount".to_string(),
                    
                    <i32 as ::mududb::types::datum::Datum>::dat_type().clone()
                    
                ),
                
            ])
        }
    )
}

pub fn mudu_result_desc_tpcc_payment() -> &'static ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc {
    static RESULT_DESC: std::sync::OnceLock<::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc> =
        std::sync::OnceLock::new();
    RESULT_DESC.get_or_init(||
        {
            ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc::new(vec![
                
                ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                    "0".to_string(),
                    
                    <i32 as ::mududb::types::datum::Datum>::dat_type().clone()
                    
                ),
                
            ])
        }
    )
}

pub fn mudu_proc_desc_tpcc_payment()  -> &'static ::mududb::contract::procedure::proc_desc::ProcDesc {
    static _PROC_DESC: std::sync::OnceLock<
        ::mududb::contract::procedure::proc_desc::ProcDesc,
    > = std::sync::OnceLock::new();
    _PROC_DESC
        .get_or_init(|| {
            ::mududb::contract::procedure::proc_desc::ProcDesc::new(
                "tpcc".to_string(),
                "tpcc_payment".to_string(),
                mudu_argv_desc_tpcc_payment().clone(),
                mudu_result_desc_tpcc_payment().clone(),
                false
            )
        })
}

mod mod_tpcc_payment {
    wit_bindgen::generate!({
        inline:
        r##"package mudu:mp2-tpcc-payment;
            world mudu-app-mp2-tpcc-payment {
                export mp2-tpcc-payment: func(param:list<u8>) -> list<u8>;
            }
        "##,
        async: true
    });

    #[allow(non_camel_case_types)]
    #[allow(unused)]
    struct GuestTpccPayment {}

    impl Guest for GuestTpccPayment {
        async fn mp2_tpcc_payment(param:Vec<u8>) -> Vec<u8> {
            super::mp2_tpcc_payment(param).await
        }
    }

    export!(GuestTpccPayment);
}
async fn mp2_tpcc_order_status(param:Vec<u8>) -> Vec<u8> {
    ::mududb::binding::procedure::procedure_invoke::invoke_procedure_async(
        param,
        mudu_inner_p2_tpcc_order_status,
    ).await
}

pub async fn mudu_inner_p2_tpcc_order_status(
    param: ::mududb::contract::procedure::procedure_param::ProcedureParam,
) -> ::mududb::common::result::RS<
    ::mududb::contract::procedure::procedure_result::ProcedureResult,
> {
    let res = tpcc_order_status(
        param.session_id(),
        
            
            ::mududb::types::datum::value_to_typed::<
                i32,
                _,
            >(&param.param_list()[0], "i32")?,
            
        
            
            ::mududb::types::datum::value_to_typed::<
                i32,
                _,
            >(&param.param_list()[1], "i32")?,
            
        
            
            ::mududb::types::datum::value_to_typed::<
                i32,
                _,
            >(&param.param_list()[2], "i32")?,
            
        
    ).await;
    match res {
        Ok(tuple) => {
            let return_list = {
                
                vec![
                    
                    ::mududb::types::datum::value_from_typed(&tuple, "String")?
                    
                ]
                
            };
            Ok(::mududb::contract::procedure::procedure_result::ProcedureResult::new(return_list))
        }
        Err(e) => Err(e),
    }
}

pub fn mudu_argv_desc_tpcc_order_status()  -> &'static ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc {
    static ARGV_DESC: std::sync::OnceLock<::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc> =
        std::sync::OnceLock::new();
    ARGV_DESC.get_or_init(||
        {
            ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc::new(vec![
                
                ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                    "warehouse_id".to_string(),
                    
                    <i32 as ::mududb::types::datum::Datum>::dat_type().clone()
                    
                ),
                
                ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                    "district_id".to_string(),
                    
                    <i32 as ::mududb::types::datum::Datum>::dat_type().clone()
                    
                ),
                
                ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                    "customer_id".to_string(),
                    
                    <i32 as ::mududb::types::datum::Datum>::dat_type().clone()
                    
                ),
                
            ])
        }
    )
}

pub fn mudu_result_desc_tpcc_order_status() -> &'static ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc {
    static RESULT_DESC: std::sync::OnceLock<::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc> =
        std::sync::OnceLock::new();
    RESULT_DESC.get_or_init(||
        {
            ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc::new(vec![
                
                ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                    "0".to_string(),
                    
                    <String as ::mududb::types::datum::Datum>::dat_type().clone()
                    
                ),
                
            ])
        }
    )
}

pub fn mudu_proc_desc_tpcc_order_status()  -> &'static ::mududb::contract::procedure::proc_desc::ProcDesc {
    static _PROC_DESC: std::sync::OnceLock<
        ::mududb::contract::procedure::proc_desc::ProcDesc,
    > = std::sync::OnceLock::new();
    _PROC_DESC
        .get_or_init(|| {
            ::mududb::contract::procedure::proc_desc::ProcDesc::new(
                "tpcc".to_string(),
                "tpcc_order_status".to_string(),
                mudu_argv_desc_tpcc_order_status().clone(),
                mudu_result_desc_tpcc_order_status().clone(),
                false
            )
        })
}

mod mod_tpcc_order_status {
    wit_bindgen::generate!({
        inline:
        r##"package mudu:mp2-tpcc-order-status;
            world mudu-app-mp2-tpcc-order-status {
                export mp2-tpcc-order-status: func(param:list<u8>) -> list<u8>;
            }
        "##,
        async: true
    });

    #[allow(non_camel_case_types)]
    #[allow(unused)]
    struct GuestTpccOrderStatus {}

    impl Guest for GuestTpccOrderStatus {
        async fn mp2_tpcc_order_status(param:Vec<u8>) -> Vec<u8> {
            super::mp2_tpcc_order_status(param).await
        }
    }

    export!(GuestTpccOrderStatus);
}
async fn mp2_tpcc_seed(param:Vec<u8>) -> Vec<u8> {
    ::mududb::binding::procedure::procedure_invoke::invoke_procedure_async(
        param,
        mudu_inner_p2_tpcc_seed,
    ).await
}

pub async fn mudu_inner_p2_tpcc_seed(
    param: ::mududb::contract::procedure::procedure_param::ProcedureParam,
) -> ::mududb::common::result::RS<
    ::mududb::contract::procedure::procedure_result::ProcedureResult,
> {
    let res = tpcc_seed(
        param.session_id(),
        
            
            ::mududb::types::datum::value_to_typed::<
                i32,
                _,
            >(&param.param_list()[0], "i32")?,
            
        
            
            ::mududb::types::datum::value_to_typed::<
                i32,
                _,
            >(&param.param_list()[1], "i32")?,
            
        
            
            ::mududb::types::datum::value_to_typed::<
                i32,
                _,
            >(&param.param_list()[2], "i32")?,
            
        
            
            ::mududb::types::datum::value_to_typed::<
                i32,
                _,
            >(&param.param_list()[3], "i32")?,
            
        
            
            ::mududb::types::datum::value_to_typed::<
                i32,
                _,
            >(&param.param_list()[4], "i32")?,
            
        
    ).await;
    match res {
        Ok(tuple) => {
            let return_list = {
                
                vec![]
                
            };
            Ok(::mududb::contract::procedure::procedure_result::ProcedureResult::new(return_list))
        }
        Err(e) => Err(e),
    }
}

pub fn mudu_argv_desc_tpcc_seed()  -> &'static ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc {
    static ARGV_DESC: std::sync::OnceLock<::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc> =
        std::sync::OnceLock::new();
    ARGV_DESC.get_or_init(||
        {
            ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc::new(vec![
                
                ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                    "warehouse_count".to_string(),
                    
                    <i32 as ::mududb::types::datum::Datum>::dat_type().clone()
                    
                ),
                
                ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                    "district_count".to_string(),
                    
                    <i32 as ::mududb::types::datum::Datum>::dat_type().clone()
                    
                ),
                
                ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                    "customer_count".to_string(),
                    
                    <i32 as ::mududb::types::datum::Datum>::dat_type().clone()
                    
                ),
                
                ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                    "item_count".to_string(),
                    
                    <i32 as ::mududb::types::datum::Datum>::dat_type().clone()
                    
                ),
                
                ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                    "initial_stock".to_string(),
                    
                    <i32 as ::mududb::types::datum::Datum>::dat_type().clone()
                    
                ),
                
            ])
        }
    )
}

pub fn mudu_result_desc_tpcc_seed() -> &'static ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc {
    static RESULT_DESC: std::sync::OnceLock<::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc> =
        std::sync::OnceLock::new();
    RESULT_DESC.get_or_init(||
        {
            ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc::new(vec![
                
            ])
        }
    )
}

pub fn mudu_proc_desc_tpcc_seed()  -> &'static ::mududb::contract::procedure::proc_desc::ProcDesc {
    static _PROC_DESC: std::sync::OnceLock<
        ::mududb::contract::procedure::proc_desc::ProcDesc,
    > = std::sync::OnceLock::new();
    _PROC_DESC
        .get_or_init(|| {
            ::mududb::contract::procedure::proc_desc::ProcDesc::new(
                "tpcc".to_string(),
                "tpcc_seed".to_string(),
                mudu_argv_desc_tpcc_seed().clone(),
                mudu_result_desc_tpcc_seed().clone(),
                false
            )
        })
}

mod mod_tpcc_seed {
    wit_bindgen::generate!({
        inline:
        r##"package mudu:mp2-tpcc-seed;
            world mudu-app-mp2-tpcc-seed {
                export mp2-tpcc-seed: func(param:list<u8>) -> list<u8>;
            }
        "##,
        async: true
    });

    #[allow(non_camel_case_types)]
    #[allow(unused)]
    struct GuestTpccSeed {}

    impl Guest for GuestTpccSeed {
        async fn mp2_tpcc_seed(param:Vec<u8>) -> Vec<u8> {
            super::mp2_tpcc_seed(param).await
        }
    }

    export!(GuestTpccSeed);
}
async fn mp2_tpcc_order_status_partitioned(param:Vec<u8>) -> Vec<u8> {
    ::mududb::binding::procedure::procedure_invoke::invoke_procedure_async(
        param,
        mudu_inner_p2_tpcc_order_status_partitioned,
    ).await
}

pub async fn mudu_inner_p2_tpcc_order_status_partitioned(
    param: ::mududb::contract::procedure::procedure_param::ProcedureParam,
) -> ::mududb::common::result::RS<
    ::mududb::contract::procedure::procedure_result::ProcedureResult,
> {
    let res = tpcc_order_status_partitioned(
        param.session_id(),
        
            
            ::mududb::types::datum::value_to_typed::<
                i32,
                _,
            >(&param.param_list()[0], "i32")?,
            
        
            
            ::mududb::types::datum::value_to_typed::<
                i32,
                _,
            >(&param.param_list()[1], "i32")?,
            
        
            
            ::mududb::types::datum::value_to_typed::<
                i32,
                _,
            >(&param.param_list()[2], "i32")?,
            
        
    ).await;
    match res {
        Ok(tuple) => {
            let return_list = {
                
                vec![
                    
                    ::mududb::types::datum::value_from_typed(&tuple, "String")?
                    
                ]
                
            };
            Ok(::mududb::contract::procedure::procedure_result::ProcedureResult::new(return_list))
        }
        Err(e) => Err(e),
    }
}

pub fn mudu_argv_desc_tpcc_order_status_partitioned()  -> &'static ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc {
    static ARGV_DESC: std::sync::OnceLock<::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc> =
        std::sync::OnceLock::new();
    ARGV_DESC.get_or_init(||
        {
            ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc::new(vec![
                
                ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                    "warehouse_id".to_string(),
                    
                    <i32 as ::mududb::types::datum::Datum>::dat_type().clone()
                    
                ),
                
                ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                    "district_id".to_string(),
                    
                    <i32 as ::mududb::types::datum::Datum>::dat_type().clone()
                    
                ),
                
                ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                    "customer_id".to_string(),
                    
                    <i32 as ::mududb::types::datum::Datum>::dat_type().clone()
                    
                ),
                
            ])
        }
    )
}

pub fn mudu_result_desc_tpcc_order_status_partitioned() -> &'static ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc {
    static RESULT_DESC: std::sync::OnceLock<::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc> =
        std::sync::OnceLock::new();
    RESULT_DESC.get_or_init(||
        {
            ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc::new(vec![
                
                ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                    "0".to_string(),
                    
                    <String as ::mududb::types::datum::Datum>::dat_type().clone()
                    
                ),
                
            ])
        }
    )
}

pub fn mudu_proc_desc_tpcc_order_status_partitioned()  -> &'static ::mududb::contract::procedure::proc_desc::ProcDesc {
    static _PROC_DESC: std::sync::OnceLock<
        ::mududb::contract::procedure::proc_desc::ProcDesc,
    > = std::sync::OnceLock::new();
    _PROC_DESC
        .get_or_init(|| {
            ::mududb::contract::procedure::proc_desc::ProcDesc::new(
                "tpcc".to_string(),
                "tpcc_order_status_partitioned".to_string(),
                mudu_argv_desc_tpcc_order_status_partitioned().clone(),
                mudu_result_desc_tpcc_order_status_partitioned().clone(),
                false
            )
        })
}

mod mod_tpcc_order_status_partitioned {
    wit_bindgen::generate!({
        inline:
        r##"package mudu:mp2-tpcc-order-status-partitioned;
            world mudu-app-mp2-tpcc-order-status-partitioned {
                export mp2-tpcc-order-status-partitioned: func(param:list<u8>) -> list<u8>;
            }
        "##,
        async: true
    });

    #[allow(non_camel_case_types)]
    #[allow(unused)]
    struct GuestTpccOrderStatusPartitioned {}

    impl Guest for GuestTpccOrderStatusPartitioned {
        async fn mp2_tpcc_order_status_partitioned(param:Vec<u8>) -> Vec<u8> {
            super::mp2_tpcc_order_status_partitioned(param).await
        }
    }

    export!(GuestTpccOrderStatusPartitioned);
}
async fn mp2_tpcc_stock_level(param:Vec<u8>) -> Vec<u8> {
    ::mududb::binding::procedure::procedure_invoke::invoke_procedure_async(
        param,
        mudu_inner_p2_tpcc_stock_level,
    ).await
}

pub async fn mudu_inner_p2_tpcc_stock_level(
    param: ::mududb::contract::procedure::procedure_param::ProcedureParam,
) -> ::mududb::common::result::RS<
    ::mududb::contract::procedure::procedure_result::ProcedureResult,
> {
    let res = tpcc_stock_level(
        param.session_id(),
        
            
            ::mududb::types::datum::value_to_typed::<
                i32,
                _,
            >(&param.param_list()[0], "i32")?,
            
        
            
            ::mududb::types::datum::value_to_typed::<
                i32,
                _,
            >(&param.param_list()[1], "i32")?,
            
        
            
            ::mududb::types::datum::value_to_typed::<
                i32,
                _,
            >(&param.param_list()[2], "i32")?,
            
        
    ).await;
    match res {
        Ok(tuple) => {
            let return_list = {
                
                vec![
                    
                    ::mududb::types::datum::value_from_typed(&tuple, "i32")?
                    
                ]
                
            };
            Ok(::mududb::contract::procedure::procedure_result::ProcedureResult::new(return_list))
        }
        Err(e) => Err(e),
    }
}

pub fn mudu_argv_desc_tpcc_stock_level()  -> &'static ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc {
    static ARGV_DESC: std::sync::OnceLock<::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc> =
        std::sync::OnceLock::new();
    ARGV_DESC.get_or_init(||
        {
            ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc::new(vec![
                
                ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                    "warehouse_id".to_string(),
                    
                    <i32 as ::mududb::types::datum::Datum>::dat_type().clone()
                    
                ),
                
                ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                    "district_id".to_string(),
                    
                    <i32 as ::mududb::types::datum::Datum>::dat_type().clone()
                    
                ),
                
                ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                    "threshold".to_string(),
                    
                    <i32 as ::mududb::types::datum::Datum>::dat_type().clone()
                    
                ),
                
            ])
        }
    )
}

pub fn mudu_result_desc_tpcc_stock_level() -> &'static ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc {
    static RESULT_DESC: std::sync::OnceLock<::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc> =
        std::sync::OnceLock::new();
    RESULT_DESC.get_or_init(||
        {
            ::mududb::contract::tuple::tuple_field_desc::TupleFieldDesc::new(vec![
                
                ::mududb::contract::tuple::datum_desc::DatumDesc::new(
                    "0".to_string(),
                    
                    <i32 as ::mududb::types::datum::Datum>::dat_type().clone()
                    
                ),
                
            ])
        }
    )
}

pub fn mudu_proc_desc_tpcc_stock_level()  -> &'static ::mududb::contract::procedure::proc_desc::ProcDesc {
    static _PROC_DESC: std::sync::OnceLock<
        ::mududb::contract::procedure::proc_desc::ProcDesc,
    > = std::sync::OnceLock::new();
    _PROC_DESC
        .get_or_init(|| {
            ::mududb::contract::procedure::proc_desc::ProcDesc::new(
                "tpcc".to_string(),
                "tpcc_stock_level".to_string(),
                mudu_argv_desc_tpcc_stock_level().clone(),
                mudu_result_desc_tpcc_stock_level().clone(),
                false
            )
        })
}

mod mod_tpcc_stock_level {
    wit_bindgen::generate!({
        inline:
        r##"package mudu:mp2-tpcc-stock-level;
            world mudu-app-mp2-tpcc-stock-level {
                export mp2-tpcc-stock-level: func(param:list<u8>) -> list<u8>;
            }
        "##,
        async: true
    });

    #[allow(non_camel_case_types)]
    #[allow(unused)]
    struct GuestTpccStockLevel {}

    impl Guest for GuestTpccStockLevel {
        async fn mp2_tpcc_stock_level(param:Vec<u8>) -> Vec<u8> {
            super::mp2_tpcc_stock_level(param).await
        }
    }

    export!(GuestTpccStockLevel);
}