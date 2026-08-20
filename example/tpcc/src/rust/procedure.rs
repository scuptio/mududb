use crate::rust::customer::object::Customer;
use crate::rust::new_order::object::NewOrder;
use crate::rust::orders::object::Orders;
use crate::rust::procedure_common::{
    customer_name, district_name, item_name, order_status_text, require_positive,
    validate_order_lines, warehouse_name,
};
use crate::rust::relation::{
    C_BALANCE, C_D_ID, C_ID, C_LAST_ORDER_ID, C_PAYMENT_CNT, C_W_ID, C_YTD_PAYMENT, D_ID,
    D_NEXT_O_ID, D_TAX, D_W_ID, D_YTD, H_AMOUNT, H_C_D_ID, H_C_ID, H_C_W_ID, H_D_ID, H_DATA, H_ID,
    H_W_ID, I_ID, I_PRICE, MONEY_6_2, MONEY_8_2, MONEY_12_2, NO_D_ID, NO_O_ID, NO_W_ID,
    O_ALL_LOCAL, O_C_ID, O_CARRIER_ID, O_D_ID, O_ENTRY_D, O_ID, O_OL_CNT, O_STATUS, O_W_ID,
    OL_AMOUNT, OL_D_ID, OL_DELIVERY_D, OL_I_ID, OL_NUMBER, OL_O_ID, OL_QUANTITY, OL_SUPPLY_W_ID,
    OL_W_ID, PH_AMOUNT, PH_C_D_ID, PH_C_ID, PH_C_W_ID, PH_D_ID, PH_DATA, PH_ID, PH_W_ID, PI_ID,
    PI_PRICE, PI_W_ID, S_I_ID, S_ORDER_CNT, S_QUANTITY, S_REMOTE_CNT, S_W_ID, S_YTD,
    TABLE_CUSTOMER, TABLE_DISTRICT, TABLE_HISTORY, TABLE_ITEM, TABLE_NEW_ORDER, TABLE_ORDER_LINE,
    TABLE_ORDERS, TABLE_STOCK, TABLE_WAREHOUSE, W_ID, W_YTD, datum_i32, datum_money, datum_text,
    key_i32, read_i32, read_money, rel_get_one, rel_insert, rel_update,
};
use bigdecimal::ToPrimitive;
use mududb::binding::universal::uni_relation::UniRelationDelta;
use mududb::common::id::OID;
use mududb::common::result::RS;
use mududb::contract::database::entity::Entity;
use mududb::contract::{sql_params, sql_stmt};
use mududb::error::ErrorCode;
use mududb::mudu::data_type::numeric::Numeric;
use mududb::mudu_error;
use mududb::sys_interface::sync_api::{mudu_command, mudu_query};

fn query_one_entity<R: Entity>(
    xid: OID,
    sql: &str,
    params: &dyn mududb::contract::database::sql_params::SQLParams,
) -> RS<R> {
    mudu_query::<R>(xid, sql_stmt!(&sql), params)?
        .next_record()?
        .ok_or_else(|| {
            mudu_error!(
                ErrorCode::EntityNotFound,
                format!("query returned no rows: {sql}")
            )
        })
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
        .ok_or_else(|| {
            mudu_error!(
                ErrorCode::EntityNotFound,
                format!("query returned no rows: {sql}")
            )
        })?;
    Ok(value as i32)
}

fn query_one_i32(
    xid: OID,
    sql: &str,
    params: &dyn mududb::contract::database::sql_params::SQLParams,
) -> RS<i32> {
    mudu_query::<i32>(xid, sql_stmt!(&sql), params)?
        .next_record()?
        .ok_or_else(|| {
            mudu_error!(
                ErrorCode::EntityNotFound,
                format!("query returned no rows: {sql}")
            )
        })
}

fn required_i32(value: &Option<i32>, field: &str) -> RS<i32> {
    value.as_ref().copied().ok_or_else(|| {
        mudu_error!(
            ErrorCode::InvalidState,
            format!("entity field is null: {field}")
        )
    })
}

/// Read a NUMERIC money column as a whole-number i32.
///
/// Money columns keep integer semantics in this workload; values may carry a
/// fractional part depending on the column scale (e.g. `NUMERIC(12,2)` renders
/// `42` as `42.00`), so the fractional part is truncated after validation.
fn required_money_i32(value: &Option<Numeric>, field: &str) -> RS<i32> {
    let numeric = value.as_ref().ok_or_else(|| {
        mudu_error!(
            ErrorCode::InvalidState,
            format!("entity field is null: {field}")
        )
    })?;
    let decimal = numeric.as_bigdecimal();
    if !decimal.is_integer() {
        return Err(mudu_error!(
            ErrorCode::InvalidState,
            format!("money field {field} has a non-zero fraction: {numeric}")
        ));
    }
    decimal.to_i32().ok_or_else(|| {
        mudu_error!(
            ErrorCode::Parse,
            format!("money field {field} does not fit into i32: {numeric}")
        )
    })
}

fn required_string(value: &Option<String>, field: &str) -> RS<String> {
    value.clone().ok_or_else(|| {
        mudu_error!(
            ErrorCode::InvalidState,
            format!("entity field is null: {field}")
        )
    })
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
                    // Keep the generated i_price within the NUMERIC(6,2) column range.
                    sql_params!(&(
                        warehouse_id,
                        item_id,
                        item_name(item_id),
                        ((item_id - 1) % 999 + 1) * 10
                    )),
                )?;
            }
        }
    } else {
        for item_id in 1..=item_count {
            mudu_command(
                xid,
                sql_stmt!(&"INSERT INTO item (i_id, i_name, i_price) VALUES (?, ?, ?)"),
                // Keep the generated i_price within the NUMERIC(6,2) column range.
                sql_params!(&(item_id, item_name(item_id), ((item_id - 1) % 999 + 1) * 10)),
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
        return Err(mudu_error!(
            ErrorCode::DomainViolation,
            "partitioned tpcc_new_order requires local supplier warehouses"
        ));
    }

    let district_key = key_i32(&[(D_W_ID, warehouse_id), (D_ID, district_id)])?;
    // Plain point read of the district row (also validates the row exists).
    rel_get_one(xid, TABLE_DISTRICT, &district_key, &[D_TAX])?;

    let customer_key = key_i32(&[
        (C_W_ID, warehouse_id),
        (C_D_ID, district_id),
        (C_ID, customer_id),
    ])?;
    // Plain point read of the customer row (also validates the row exists).
    rel_get_one(xid, TABLE_CUSTOMER, &customer_key, &[C_BALANCE])?;

    // Allocate the next order id with an atomic increment: the delta is
    // evaluated on the latest committed d_next_o_id under the row lock, so
    // concurrent new-order transactions can no longer read the same stale
    // value and collide on the orders/new_order primary key.
    rel_update(
        xid,
        TABLE_DISTRICT,
        &district_key,
        &[],
        &[UniRelationDelta::add(D_NEXT_O_ID, datum_i32(1)?)],
    )?;
    // Read-your-writes: this observes the increment staged above, so the
    // allocated order id is the pre-increment value.
    let next_order_id = read_i32(
        &rel_get_one(xid, TABLE_DISTRICT, &district_key, &[D_NEXT_O_ID])?,
        0,
        "district.d_next_o_id",
    )? - 1;
    let all_local = supplier_warehouse_ids
        .iter()
        .all(|&supplier_warehouse_id| supplier_warehouse_id == warehouse_id);
    let entry_d = format!("xid-{xid}-o{next_order_id}");

    rel_insert(
        xid,
        TABLE_ORDERS,
        &key_i32(&[
            (O_W_ID, warehouse_id),
            (O_D_ID, district_id),
            (O_ID, next_order_id),
        ])?,
        &[
            (O_C_ID, datum_i32(customer_id)?),
            (O_ENTRY_D, datum_text(&entry_d)?),
            (O_CARRIER_ID, datum_i32(0)?),
            (O_OL_CNT, datum_i32(item_ids.len() as i32)?),
            (O_ALL_LOCAL, datum_i32(if all_local { 1 } else { 0 })?),
            (O_STATUS, datum_text("NEW")?),
        ],
    )?;
    rel_insert(
        xid,
        TABLE_NEW_ORDER,
        &key_i32(&[
            (NO_W_ID, warehouse_id),
            (NO_D_ID, district_id),
            (NO_O_ID, next_order_id),
        ])?,
        &[],
    )?;

    let mut total_quantity = 0;
    let mut total_amount = 0;
    for (idx, ((&item_id, &supplier_warehouse_id), &quantity)) in item_ids
        .iter()
        .zip(supplier_warehouse_ids.iter())
        .zip(quantities.iter())
        .enumerate()
    {
        let (item_key, item_price_attr) = if warehouse_partitioned {
            (
                key_i32(&[(PI_W_ID, warehouse_id), (PI_ID, item_id)])?,
                PI_PRICE,
            )
        } else {
            (key_i32(&[(I_ID, item_id)])?, I_PRICE)
        };
        let item_row = rel_get_one(xid, TABLE_ITEM, &item_key, &[item_price_attr])?;
        let item_price = required_money_i32(
            &Some(read_money(&item_row, 0, MONEY_6_2, "item.i_price")?),
            "item.i_price",
        )?;
        // s_quantity's conditional restock commutes with any other such
        // update when written as `((current - 10 - q) mod 91) + 10`, so it
        // is issued as a deferred conditional-restock delta evaluated
        // atomically at commit apply time — no statement lock and no
        // old-value read. s_ytd / s_order_cnt / s_remote_cnt are
        // unconditional increments and commute as well, so they go through
        // the same lock-free deferred path.
        let stock_key = key_i32(&[(S_W_ID, supplier_warehouse_id), (S_I_ID, item_id)])?;
        let is_remote = supplier_warehouse_id != warehouse_id;
        let amount = item_price * quantity;

        let mut deltas = vec![
            UniRelationDelta::sub_wrap(S_QUANTITY, quantity as i64, 10, 91),
            UniRelationDelta::add_deferred(S_YTD, datum_i32(quantity)?),
            UniRelationDelta::add_deferred(S_ORDER_CNT, datum_i32(1)?),
        ];
        if is_remote {
            deltas.push(UniRelationDelta::add_deferred(S_REMOTE_CNT, datum_i32(1)?));
        }
        rel_update(xid, TABLE_STOCK, &stock_key, &[], &deltas)?;
        rel_insert(
            xid,
            TABLE_ORDER_LINE,
            &key_i32(&[
                (OL_W_ID, warehouse_id),
                (OL_D_ID, district_id),
                (OL_O_ID, next_order_id),
                (OL_NUMBER, idx as i32 + 1),
            ])?,
            &[
                (OL_I_ID, datum_i32(item_id)?),
                (OL_SUPPLY_W_ID, datum_i32(supplier_warehouse_id)?),
                (OL_DELIVERY_D, datum_text("")?),
                (OL_QUANTITY, datum_i32(quantity)?),
                (OL_AMOUNT, datum_money(amount, MONEY_8_2)?),
            ],
        )?;
        total_quantity += quantity;
        total_amount += amount;
    }
    rel_update(
        xid,
        TABLE_CUSTOMER,
        &customer_key,
        &[(C_LAST_ORDER_ID, datum_i32(next_order_id)?)],
        &[],
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

    // The customer balance is still read to report the post-payment balance.
    // The write side below uses delta updates (`col = col +|- datum`), so the
    // warehouse/district base-value reads are gone and no concurrent payment
    // can be lost; the reported balance comes from this pre-update read and
    // may lag a concurrent payment.
    let customer_key = key_i32(&[
        (C_W_ID, warehouse_id),
        (C_D_ID, district_id),
        (C_ID, customer_id),
    ])?;
    let customer_row = rel_get_one(xid, TABLE_CUSTOMER, &customer_key, &[C_BALANCE])?;
    let next_c_balance = required_money_i32(
        &Some(read_money(
            &customer_row,
            0,
            MONEY_12_2,
            "customer.c_balance",
        )?),
        "customer.c_balance",
    )? - amount;

    let history_id = mududb::sys::random::next_uuid_v4_string();
    let history_data = format!("payment warehouse={warehouse_id} district={district_id}");
    if warehouse_partitioned {
        rel_insert(
            xid,
            TABLE_HISTORY,
            &[
                (PH_W_ID, datum_i32(warehouse_id)?),
                (PH_ID, datum_text(&history_id)?),
            ],
            &[
                (PH_C_ID, datum_i32(customer_id)?),
                (PH_C_D_ID, datum_i32(district_id)?),
                (PH_C_W_ID, datum_i32(warehouse_id)?),
                (PH_D_ID, datum_i32(district_id)?),
                (PH_AMOUNT, datum_money(amount, MONEY_6_2)?),
                (PH_DATA, datum_text(&history_data)?),
            ],
        )?;
    } else {
        rel_insert(
            xid,
            TABLE_HISTORY,
            &[(H_ID, datum_text(&history_id)?)],
            &[
                (H_C_ID, datum_i32(customer_id)?),
                (H_C_D_ID, datum_i32(district_id)?),
                (H_C_W_ID, datum_i32(warehouse_id)?),
                (H_D_ID, datum_i32(district_id)?),
                (H_W_ID, datum_i32(warehouse_id)?),
                (H_AMOUNT, datum_money(amount, MONEY_6_2)?),
                (H_DATA, datum_text(&history_data)?),
            ],
        )?;
    }
    // Delta updates evaluate on the latest tuple read under the statement
    // lock, making concurrent payments atomic. The hottest row (warehouse,
    // one row per warehouse for the whole table) is updated last so its lock
    // escort tail covers only the commit critical section.
    rel_update(
        xid,
        TABLE_CUSTOMER,
        &customer_key,
        &[],
        &[
            UniRelationDelta::sub(C_BALANCE, datum_money(amount, MONEY_12_2)?),
            UniRelationDelta::add(C_YTD_PAYMENT, datum_money(amount, MONEY_12_2)?),
            UniRelationDelta::add(C_PAYMENT_CNT, datum_i32(1)?),
        ],
    )?;
    rel_update(
        xid,
        TABLE_DISTRICT,
        &key_i32(&[(D_W_ID, warehouse_id), (D_ID, district_id)])?,
        &[],
        &[UniRelationDelta::add(
            D_YTD,
            datum_money(amount, MONEY_12_2)?,
        )],
    )?;
    rel_update(
        xid,
        TABLE_WAREHOUSE,
        &key_i32(&[(W_ID, warehouse_id)])?,
        &[],
        &[UniRelationDelta::add(
            W_YTD,
            datum_money(amount, MONEY_12_2)?,
        )],
    )?;
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
    .ok_or_else(|| {
        mudu_error!(
            ErrorCode::EntityNotFound,
            "delivery found no pending new_order rows"
        )
    })?;
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

/// Hot-row contention injector: increments one of the K per-warehouse
/// hotspot rows (`tpcc_hotspot`, created client-side by the benchmark).
/// Runs as its own tiny transaction right after a TPC-C op.
fn tpcc_hotspot_hit_inner(xid: OID, warehouse_id: i32, hot_id: i32) -> RS<i32> {
    require_positive("warehouse_id", warehouse_id)?;
    require_positive("hot_id", hot_id)?;
    mudu_command(
        xid,
        sql_stmt!(
            &"UPDATE tpcc_hotspot SET h_counter = h_counter + 1 WHERE h_w_id = ? AND h_id = ?"
        ),
        sql_params!(&(warehouse_id, hot_id)),
    )?;
    query_one_i32(
        xid,
        "SELECT h_counter FROM tpcc_hotspot WHERE h_w_id = ? AND h_id = ?",
        sql_params!(&(warehouse_id, hot_id)),
    )
}

/**mudu-proc**/
pub fn tpcc_hotspot_hit(xid: OID, warehouse_id: i32, hot_id: i32) -> RS<i32> {
    tpcc_hotspot_hit_inner(xid, warehouse_id, hot_id)
}

/**mudu-proc**/
pub fn tpcc_hotspot_hit_partitioned(xid: OID, warehouse_id: i32, hot_id: i32) -> RS<i32> {
    tpcc_hotspot_hit_inner(xid, warehouse_id, hot_id)
}

// Miri cannot execute FFI calls into SQLite (via rusqlite), so skip
// these tests under Miri. They are still exercised by normal `cargo test`.
#[cfg(test)]
mod tests {
    use super::{
        tpcc_delivery, tpcc_new_order, tpcc_order_status, tpcc_payment, tpcc_seed, tpcc_stock_level,
    };
    use crate::test_lock;
    use mududb::contract::{sql_params, sql_stmt};
    use mududb::sys::env_var::temp_dir;
    use mududb::sys::time::system_time_now;
    use mududb::sys_interface::sync_api::{mudu_batch, mudu_close, mudu_open};
    use std::path::PathBuf;
    use std::time::UNIX_EPOCH;

    fn temp_db_path(name: &str) -> PathBuf {
        let suffix = system_time_now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before unix epoch")
            .as_nanos();
        temp_dir().join(format!("tpcc_sql_{name}_{suffix}.db"))
    }

    fn init_schema(xid: u128) {
        let ddl = include_str!("../../sql/ddl.sql");
        let init = include_str!("../../sql/init.sql");
        mudu_batch(xid, sql_stmt!(&ddl), sql_params!(&())).unwrap();
        mudu_batch(xid, sql_stmt!(&init), sql_params!(&())).unwrap();
    }

    fn run_sync_roundtrip(db_name: &str) {
        let db_path = temp_db_path(db_name);
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

    #[test]
    #[cfg_attr(miri, ignore)]
    fn tpcc_sync_procedures_roundtrip_against_standalone_adapter() {
        let _guard = test_lock().lock().unwrap();
        crate::rust::relation::reset_relation_support_for_test();
        run_sync_roundtrip("sync");
    }

    // Same roundtrip, but with the relation syscalls forced onto the SQL
    // fallback path used by drivers without relation-syscall support.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn tpcc_sync_procedures_roundtrip_through_relation_sql_fallback() {
        let _guard = test_lock().lock().unwrap();
        crate::rust::relation::force_relation_sql_fallback_for_test();
        run_sync_roundtrip("sync_sql_fallback");
        crate::rust::relation::reset_relation_support_for_test();
    }
}
