use crate::rust::customer::object::Customer;
use crate::rust::district::object::District;
use crate::rust::item::object::Item;
use crate::rust::new_order::object::NewOrder;
use crate::rust::orders::object::Orders;
use crate::rust::procedure_common::{
    customer_name, district_name, item_name, order_status_text, require_positive,
    validate_order_lines, warehouse_name,
};
use crate::rust::stock::object::Stock;
use crate::rust::warehouse::object::Warehouse;
use mududb::common::id::OID;
use mududb::common::result::RS;
use mududb::contract::database::entity::Entity;
use mududb::contract::{sql_params, sql_stmt};
use mududb::error::ec::EC::MuduError;
use mududb::m_error;
use mududb::sys_interface::sync_api::{mudu_command, mudu_query};

fn query_one_entity<R: Entity>(
    xid: OID,
    sql: &str,
    params: &dyn mududb::contract::database::sql_params::SQLParams,
) -> RS<R> {
    mudu_query::<R>(xid, sql_stmt!(&sql), params)?
        .next_record()?
        .ok_or_else(|| m_error!(MuduError, format!("query returned no rows: {sql}")))
}

fn query_entities<R: Entity>(
    xid: OID,
    sql: &str,
    params: &dyn mududb::contract::database::sql_params::SQLParams,
) -> RS<Vec<R>> {
    let mut result_set = mudu_query::<R>(xid, sql_stmt!(&sql), params)?;
    let mut values = Vec::new();
    while let Some(value) = result_set.next_record()? {
        values.push(value);
    }
    Ok(values)
}

fn query_count_i32(
    xid: OID,
    sql: &str,
    params: &dyn mududb::contract::database::sql_params::SQLParams,
) -> RS<i32> {
    let value = mudu_query::<i64>(xid, sql_stmt!(&sql), params)?
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

fn tpcc_seed_inner(
    xid: OID,
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
                )?;
            }
        }
    } else {
        for item_id in 1..=item_count {
            mudu_command(
                xid,
                sql_stmt!(&"INSERT INTO item (i_id, i_name, i_price) VALUES (?, ?, ?)"),
                sql_params!(&(item_id, item_name(item_id), item_id * 10)),
            )?;
        }
    }
    for warehouse_id in 1..=warehouse_count {
        mudu_command(
            xid,
            sql_stmt!(&"INSERT INTO warehouse (w_id, w_name, w_tax, w_ytd) VALUES (?, ?, ?, 0)"),
            sql_params!(&(warehouse_id, warehouse_name(warehouse_id), warehouse_id % 7)),
        )?;
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
            )?;
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
                )?;
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
            )?;
        }
    }
    Ok(())
}

struct TpccNewOrderRequest {
    xid: OID,
    warehouse_id: i32,
    district_id: i32,
    customer_id: i32,
    item_ids: Vec<i32>,
    supplier_warehouse_ids: Vec<i32>,
    quantities: Vec<i32>,
    warehouse_partitioned: bool,
}

fn tpcc_new_order_inner(request: TpccNewOrderRequest) -> RS<String> {
    let TpccNewOrderRequest {
        xid,
        warehouse_id,
        district_id,
        customer_id,
        item_ids,
        supplier_warehouse_ids,
        quantities,
        warehouse_partitioned,
    } = request;
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
    )?;
    let next_order_id = required_i32(district.get_d_next_o_id(), "district.d_next_o_id")?;
    let next_d_next_o_id = next_order_id + 1;

    query_one_entity::<Customer>(
        xid,
        "SELECT c_id, c_d_id, c_w_id, c_first, c_last, c_discount, c_credit, c_balance, c_ytd_payment, c_payment_cnt, c_delivery_cnt, c_last_order_id FROM customer WHERE c_w_id = ? AND c_d_id = ? AND c_id = ?",
        sql_params!(&(warehouse_id, district_id, customer_id)),
    )?;

    mudu_command(
        xid,
        sql_stmt!(&"UPDATE district SET d_next_o_id = ? WHERE d_w_id = ? AND d_id = ?"),
        sql_params!(&(next_d_next_o_id, warehouse_id, district_id)),
    )?;
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
    )?;
    mudu_command(
        xid,
        sql_stmt!(&"INSERT INTO new_order (no_o_id, no_d_id, no_w_id) VALUES (?, ?, ?)"),
        sql_params!(&(next_order_id, district_id, warehouse_id)),
    )?;

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
            )?
        } else {
            query_one_entity::<Item>(
                xid,
                "SELECT i_id, i_name, i_price FROM item WHERE i_id = ?",
                sql_params!(&(item_id,)),
            )?
        };
        let item_price = required_i32(item.get_i_price(), "item.i_price")?;
        let stock = query_one_entity::<Stock>(
            xid,
            "SELECT s_i_id, s_w_id, s_quantity, s_ytd, s_order_cnt, s_remote_cnt FROM stock WHERE s_w_id = ? AND s_i_id = ?",
            sql_params!(&(supplier_warehouse_id, item_id)),
        )?;
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
        )?;
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
        )?;
        total_quantity += quantity;
        total_amount += amount;
    }
    mudu_command(
        xid,
        sql_stmt!(
            &"UPDATE customer SET c_last_order_id = ? WHERE c_w_id = ? AND c_d_id = ? AND c_id = ?"
        ),
        sql_params!(&(next_order_id, warehouse_id, district_id, customer_id)),
    )?;

    Ok(order_status_text(
        next_order_id,
        item_ids.len(),
        total_quantity,
        total_amount,
        all_local,
        "NEW",
    ))
}

fn tpcc_payment_inner(
    xid: OID,
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
    )?;
    let district = query_one_entity::<District>(
        xid,
        "SELECT d_id, d_w_id, d_name, d_tax, d_ytd, d_next_o_id, d_last_delivery_o_id FROM district WHERE d_w_id = ? AND d_id = ?",
        sql_params!(&(warehouse_id, district_id)),
    )?;
    let customer = query_one_entity::<Customer>(
        xid,
        "SELECT c_id, c_d_id, c_w_id, c_first, c_last, c_discount, c_credit, c_balance, c_ytd_payment, c_payment_cnt, c_delivery_cnt, c_last_order_id FROM customer WHERE c_w_id = ? AND c_d_id = ? AND c_id = ?",
        sql_params!(&(warehouse_id, district_id, customer_id)),
    )?;
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
    )?;
    mudu_command(
        xid,
        sql_stmt!(&"UPDATE district SET d_ytd = ? WHERE d_w_id = ? AND d_id = ?"),
        sql_params!(&(next_d_ytd, warehouse_id, district_id)),
    )?;
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
    )?;
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
        )?;
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
        )?;
    }
    Ok(next_c_balance)
}

/**mudu-proc**/
pub fn tpcc_seed(
    xid: OID,
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
    )
}

/**mudu-proc**/
pub fn tpcc_seed_partitioned(
    xid: OID,
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
    )
}

/**mudu-proc**/
pub fn tpcc_new_order(
    xid: OID,
    warehouse_id: i32,
    district_id: i32,
    customer_id: i32,
    item_ids: Vec<i32>,
    supplier_warehouse_ids: Vec<i32>,
    quantities: Vec<i32>,
) -> RS<String> {
    tpcc_new_order_inner(TpccNewOrderRequest {
        xid,
        warehouse_id,
        district_id,
        customer_id,
        item_ids,
        supplier_warehouse_ids,
        quantities,
        warehouse_partitioned: false,
    })
}

/**mudu-proc**/
pub fn tpcc_new_order_partitioned(
    xid: OID,
    warehouse_id: i32,
    district_id: i32,
    customer_id: i32,
    item_ids: Vec<i32>,
    supplier_warehouse_ids: Vec<i32>,
    quantities: Vec<i32>,
) -> RS<String> {
    tpcc_new_order_inner(TpccNewOrderRequest {
        xid,
        warehouse_id,
        district_id,
        customer_id,
        item_ids,
        supplier_warehouse_ids,
        quantities,
        warehouse_partitioned: true,
    })
}

/**mudu-proc**/
pub fn tpcc_payment(
    xid: OID,
    warehouse_id: i32,
    district_id: i32,
    customer_id: i32,
    amount: i32,
) -> RS<i32> {
    tpcc_payment_inner(xid, warehouse_id, district_id, customer_id, amount, false)
}

/**mudu-proc**/
pub fn tpcc_payment_partitioned(
    xid: OID,
    warehouse_id: i32,
    district_id: i32,
    customer_id: i32,
    amount: i32,
) -> RS<i32> {
    tpcc_payment_inner(xid, warehouse_id, district_id, customer_id, amount, true)
}

/**mudu-proc**/
pub fn tpcc_order_status(
    xid: OID,
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
    )?;
    let order_id = required_i32(customer.get_c_last_order_id(), "customer.c_last_order_id")?;
    let order = query_one_entity::<Orders>(
        xid,
        "SELECT o_id, o_d_id, o_w_id, o_c_id, o_entry_d, o_carrier_id, o_ol_cnt, o_all_local, o_status FROM orders WHERE o_w_id = ? AND o_d_id = ? AND o_id = ?",
        sql_params!(&(warehouse_id, district_id, order_id)),
    )?;
    required_string(order.get_o_status(), "orders.o_status")
}

/**mudu-proc**/
pub fn tpcc_order_status_partitioned(
    xid: OID,
    warehouse_id: i32,
    district_id: i32,
    customer_id: i32,
) -> RS<String> {
    tpcc_order_status(xid, warehouse_id, district_id, customer_id)
}

/**mudu-proc**/
pub fn tpcc_delivery(xid: OID, warehouse_id: i32, district_id: i32, carrier_id: i32) -> RS<String> {
    require_positive("warehouse_id", warehouse_id)?;
    require_positive("district_id", district_id)?;
    require_positive("carrier_id", carrier_id)?;

    let order_id = query_entities::<NewOrder>(
        xid,
        "SELECT no_o_id, no_d_id, no_w_id FROM new_order WHERE no_w_id = ? AND no_d_id = ?",
        sql_params!(&(warehouse_id, district_id)),
    )?
    .into_iter()
    .filter_map(|row| row.get_no_o_id().as_ref().copied())
    .min()
    .ok_or_else(|| m_error!(MuduError, "delivery found no pending new_order rows"))?;
    mudu_command(
        xid,
        sql_stmt!(&"DELETE FROM new_order WHERE no_w_id = ? AND no_d_id = ? AND no_o_id = ?"),
        sql_params!(&(warehouse_id, district_id, order_id)),
    )?;
    mudu_command(
        xid,
        sql_stmt!(&"UPDATE district SET d_last_delivery_o_id = ? WHERE d_w_id = ? AND d_id = ?"),
        sql_params!(&(order_id, warehouse_id, district_id)),
    )?;
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
    )?;
    let order = query_one_entity::<Orders>(
        xid,
        "SELECT o_id, o_d_id, o_w_id, o_c_id, o_entry_d, o_carrier_id, o_ol_cnt, o_all_local, o_status FROM orders WHERE o_w_id = ? AND o_d_id = ? AND o_id = ?",
        sql_params!(&(warehouse_id, district_id, order_id)),
    )?;
    let customer_id = required_i32(order.get_o_c_id(), "orders.o_c_id")?;
    let customer = query_one_entity::<Customer>(
        xid,
        "SELECT c_id, c_d_id, c_w_id, c_first, c_last, c_discount, c_credit, c_balance, c_ytd_payment, c_payment_cnt, c_delivery_cnt, c_last_order_id FROM customer WHERE c_w_id = ? AND c_d_id = ? AND c_id = ?",
        sql_params!(&(warehouse_id, district_id, customer_id)),
    )?;
    let next_delivery_cnt =
        required_i32(customer.get_c_delivery_cnt(), "customer.c_delivery_cnt")? + 1;
    mudu_command(
        xid,
        sql_stmt!(
            &"UPDATE customer SET c_delivery_cnt = ? WHERE c_w_id = ? AND c_d_id = ? AND c_id = ?"
        ),
        sql_params!(&(next_delivery_cnt, warehouse_id, district_id, customer_id)),
    )?;
    Ok(format!("delivered order={order_id} carrier={carrier_id}"))
}

/**mudu-proc**/
pub fn tpcc_delivery_partitioned(
    xid: OID,
    warehouse_id: i32,
    district_id: i32,
    carrier_id: i32,
) -> RS<String> {
    tpcc_delivery(xid, warehouse_id, district_id, carrier_id)
}

/**mudu-proc**/
pub fn tpcc_stock_level(xid: OID, warehouse_id: i32, district_id: i32, threshold: i32) -> RS<i32> {
    require_positive("warehouse_id", warehouse_id)?;
    require_positive("district_id", district_id)?;
    require_positive("threshold", threshold)?;
    query_count_i32(
        xid,
        "SELECT COUNT(*) AS field_i64 FROM stock WHERE s_w_id = ? AND s_quantity < ?",
        sql_params!(&(warehouse_id, threshold)),
    )
}

/**mudu-proc**/
pub fn tpcc_stock_level_partitioned(
    xid: OID,
    warehouse_id: i32,
    district_id: i32,
    threshold: i32,
) -> RS<i32> {
    tpcc_stock_level(xid, warehouse_id, district_id, threshold)
}

#[cfg(test)]
mod tests {
    use super::{
        tpcc_delivery, tpcc_new_order, tpcc_order_status, tpcc_payment, tpcc_seed, tpcc_stock_level,
    };
    use crate::test_lock;
    use mududb::contract::{sql_params, sql_stmt};
    use mududb::sys_interface::sync_api::{mudu_batch, mudu_close, mudu_open};
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
    fn tpcc_sync_procedures_roundtrip_against_standalone_adapter() {
        let _guard = test_lock().lock().unwrap_or_else(|err| err.into_inner());
        let db_path = temp_db_path("sync");
        mudu_adapter::config::reset_db_path_override_for_test();
        mudu_adapter::syscall::set_db_path(&db_path);

        let xid = mudu_open().unwrap();
        init_schema(xid);
        tpcc_seed(xid, 1, 2, 4, 5, 20).unwrap();

        let order =
            tpcc_new_order(xid, 1, 1, 1, vec![2, 4, 5], vec![1, 1, 1], vec![3, 2, 1]).unwrap();
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

        mudu_close(xid).unwrap();
    }
}
