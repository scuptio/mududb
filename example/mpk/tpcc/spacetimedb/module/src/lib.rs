//! TPC-C benchmark module for SpacetimeDB.
//!
//! This is a line-by-line port of the mududb TPC-C procedures in
//! `example/tpcc/src/rust/procedure.rs` to the SpacetimeDB table/reducer API.
//! SpacetimeDB 1.x has no composite primary keys, so composite keys are
//! encoded into single `u64` columns (see the `*_pk` helpers below).

use spacetimedb::{reducer, table, ReducerContext, Table};

// ---------------------------------------------------------------------------
// Composite key encoding.
//
// Spacing constants are powers of 10 chosen to be compatible with the
// benchmark data scale (districts <= 100, customers/items configurable,
// order ids grow monotonically):
//   district pk    = w_id * 1_000 + d_id                          (d_id < 1_000)
//   customer pk    = district pk * 10_000_000 + c_id              (c_id < 1e7)
//   stock pk       = w_id * 10_000_000 + i_id                     (i_id < 1e7)
//   orders pk      = district pk * 100_000_000_000 + o_id         (o_id < 1e11)
//   new_order pk   = same encoding as orders pk
//   order_line pk  = orders pk * 100 + ol_number                  (ol_number < 100)
// ---------------------------------------------------------------------------

const DISTRICT_SPACING: u64 = 1_000;
const CUSTOMER_SPACING: u64 = 10_000_000;
const ITEM_SPACING: u64 = 10_000_000;
const ORDER_SPACING: u64 = 100_000_000_000;
const ORDER_LINE_SPACING: u64 = 100;

fn district_pk(warehouse_id: i32, district_id: i32) -> u64 {
    warehouse_id as u64 * DISTRICT_SPACING + district_id as u64
}

fn customer_pk(warehouse_id: i32, district_id: i32, customer_id: i32) -> u64 {
    district_pk(warehouse_id, district_id) * CUSTOMER_SPACING + customer_id as u64
}

fn stock_pk(warehouse_id: i32, item_id: i32) -> u64 {
    warehouse_id as u64 * ITEM_SPACING + item_id as u64
}

fn order_pk(warehouse_id: i32, district_id: i32, order_id: i32) -> u64 {
    district_pk(warehouse_id, district_id) * ORDER_SPACING + order_id as u64
}

fn order_line_pk(warehouse_id: i32, district_id: i32, order_id: i32, line_number: i32) -> u64 {
    order_pk(warehouse_id, district_id, order_id) * ORDER_LINE_SPACING + line_number as u64
}

/// Matches the `initial_stock` argument (100) used by the other backends.
const INITIAL_STOCK: i32 = 100;

// ---------------------------------------------------------------------------
// Tables (mirrors example/tpcc/sql/ddl.sql, plus encoded pk columns).
// ---------------------------------------------------------------------------

#[table(name = warehouse, public)]
pub struct Warehouse {
    #[primary_key]
    w_id: i32,
    w_name: String,
    w_tax: i32,
    w_ytd: i32,
}

#[table(name = district, public)]
pub struct District {
    /// Encoded (d_w_id, d_id) primary key.
    #[primary_key]
    pk: u64,
    d_id: i32,
    d_w_id: i32,
    d_name: String,
    d_tax: i32,
    d_ytd: i32,
    d_next_o_id: i32,
    d_last_delivery_o_id: i32,
}

#[table(name = customer, public)]
pub struct Customer {
    /// Encoded (c_w_id, c_d_id, c_id) primary key.
    #[primary_key]
    pk: u64,
    c_id: i32,
    c_d_id: i32,
    c_w_id: i32,
    c_first: String,
    c_last: String,
    c_discount: i32,
    c_credit: String,
    c_balance: i32,
    c_ytd_payment: i32,
    c_payment_cnt: i32,
    c_delivery_cnt: i32,
    c_last_order_id: i32,
}

#[table(name = item, public)]
pub struct Item {
    #[primary_key]
    i_id: i32,
    i_name: String,
    i_price: i32,
}

#[table(name = stock, public)]
pub struct Stock {
    /// Encoded (s_w_id, s_i_id) primary key.
    #[primary_key]
    pk: u64,
    /// Btree index for the stock_level range count (`s_w_id = ?`).
    #[index(btree)]
    s_w_id: i32,
    s_i_id: i32,
    s_quantity: i32,
    s_ytd: i32,
    s_order_cnt: i32,
    s_remote_cnt: i32,
}

#[table(name = orders, public)]
pub struct Orders {
    /// Encoded (o_w_id, o_d_id, o_id) primary key.
    #[primary_key]
    pk: u64,
    o_id: i32,
    o_d_id: i32,
    o_w_id: i32,
    o_c_id: i32,
    o_entry_d: String,
    o_carrier_id: i32,
    o_ol_cnt: i32,
    o_all_local: i32,
    o_status: String,
}

#[table(name = new_order, public)]
pub struct NewOrder {
    /// Encoded (no_w_id, no_d_id, no_o_id) primary key.
    #[primary_key]
    pk: u64,
    /// Btree index on the encoded (no_w_id, no_d_id) prefix so delivery can
    /// find the oldest pending order of a district without a full scan.
    #[index(btree)]
    no_wd: u64,
    no_o_id: i32,
    no_d_id: i32,
    no_w_id: i32,
}

#[table(name = order_line, public)]
pub struct OrderLine {
    /// Encoded (ol_w_id, ol_d_id, ol_o_id, ol_number) primary key.
    #[primary_key]
    pk: u64,
    ol_o_id: i32,
    ol_d_id: i32,
    ol_w_id: i32,
    ol_number: i32,
    ol_i_id: i32,
    ol_supply_w_id: i32,
    ol_delivery_d: String,
    ol_quantity: i32,
    ol_amount: i32,
}

#[table(name = history, public)]
pub struct History {
    /// Surrogate key; the original schema uses a uuid `h_id` string, which is
    /// kept as a regular column. Nothing in the workload reads history back.
    #[auto_inc]
    #[primary_key]
    h_seq: u64,
    h_id: String,
    h_c_id: i32,
    h_c_d_id: i32,
    h_c_w_id: i32,
    h_d_id: i32,
    h_w_id: i32,
    h_amount: i32,
    h_data: String,
}

// ---------------------------------------------------------------------------
// Shared helpers (copied from example/tpcc/src/rust/procedure_common.rs so
// that seed data is byte-identical with the other backends).
// ---------------------------------------------------------------------------

fn require_positive(name: &str, value: i32) -> Result<(), String> {
    if value <= 0 {
        return Err(format!("{name} must be positive, got {value}"));
    }
    Ok(())
}

fn customer_name(warehouse_id: i32, district_id: i32, customer_id: i32) -> (String, String) {
    (
        format!("Customer{warehouse_id}_{district_id}_{customer_id}"),
        format!("Last{customer_id}"),
    )
}

fn district_name(warehouse_id: i32, district_id: i32) -> String {
    format!("District{warehouse_id}_{district_id}")
}

fn warehouse_name(warehouse_id: i32) -> String {
    format!("Warehouse{warehouse_id}")
}

fn item_name(item_id: i32) -> String {
    format!("Item{item_id}")
}

fn validate_order_lines(
    item_ids: &[i32],
    supplier_warehouse_ids: &[i32],
    quantities: &[i32],
) -> Result<(), String> {
    if item_ids.is_empty() {
        return Err("new_order requires at least one item".to_string());
    }
    if item_ids.len() != supplier_warehouse_ids.len() {
        return Err(format!(
            "item_ids and supplier_warehouse_ids length mismatch: {} != {}",
            item_ids.len(),
            supplier_warehouse_ids.len()
        ));
    }
    if item_ids.len() != quantities.len() {
        return Err(format!(
            "item_ids and quantities length mismatch: {} != {}",
            item_ids.len(),
            quantities.len()
        ));
    }
    for &item_id in item_ids {
        require_positive("item_id", item_id)?;
    }
    for &supplier_warehouse_id in supplier_warehouse_ids {
        require_positive("supplier_warehouse_id", supplier_warehouse_id)?;
    }
    for &quantity in quantities {
        require_positive("quantity", quantity)?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Reducers (port of example/tpcc/src/rust/procedure.rs).
//
// Every reducer takes a trailing `op_index` so the benchmark client can match
// reducer callbacks to its own invocations. Returning `Err` aborts and rolls
// back the transaction, which the client counts as an aborted operation.
// ---------------------------------------------------------------------------

/// Seeds a single warehouse (plus the shared item table when
/// `warehouse_id == 1`). The client calls this once per warehouse so that a
/// single reducer transaction stays small enough to avoid OutOfEnergy.
#[reducer]
pub fn tpcc_seed(
    ctx: &ReducerContext,
    warehouse_id: i32,
    district_count: i32,
    customer_count: i32,
    item_count: i32,
    op_index: u64,
) -> Result<(), String> {
    let _ = op_index;
    require_positive("warehouse_id", warehouse_id)?;
    require_positive("district_count", district_count)?;
    require_positive("customer_count", customer_count)?;
    require_positive("item_count", item_count)?;

    if warehouse_id == 1 {
        for item_id in 1..=item_count {
            ctx.db.item().insert(Item {
                i_id: item_id,
                i_name: item_name(item_id),
                i_price: item_id * 10,
            });
        }
    }
    ctx.db.warehouse().insert(Warehouse {
        w_id: warehouse_id,
        w_name: warehouse_name(warehouse_id),
        w_tax: warehouse_id % 7,
        w_ytd: 0,
    });
    for district_id in 1..=district_count {
        ctx.db.district().insert(District {
            pk: district_pk(warehouse_id, district_id),
            d_id: district_id,
            d_w_id: warehouse_id,
            d_name: district_name(warehouse_id, district_id),
            d_tax: district_id % 9,
            d_ytd: 0,
            d_next_o_id: 1,
            d_last_delivery_o_id: 0,
        });
        for customer_id in 1..=customer_count {
            let (first, last) = customer_name(warehouse_id, district_id, customer_id);
            ctx.db.customer().insert(Customer {
                pk: customer_pk(warehouse_id, district_id, customer_id),
                c_id: customer_id,
                c_d_id: district_id,
                c_w_id: warehouse_id,
                c_first: first,
                c_last: last,
                c_discount: customer_id % 5,
                c_credit: "GC".to_string(),
                c_balance: 0,
                c_ytd_payment: 0,
                c_payment_cnt: 0,
                c_delivery_cnt: 0,
                c_last_order_id: 0,
            });
        }
    }
    for item_id in 1..=item_count {
        ctx.db.stock().insert(Stock {
            pk: stock_pk(warehouse_id, item_id),
            s_w_id: warehouse_id,
            s_i_id: item_id,
            s_quantity: INITIAL_STOCK,
            s_ytd: 0,
            s_order_cnt: 0,
            s_remote_cnt: 0,
        });
    }
    Ok(())
}

#[reducer]
pub fn tpcc_new_order(
    ctx: &ReducerContext,
    warehouse_id: i32,
    district_id: i32,
    customer_id: i32,
    item_ids: Vec<i32>,
    supplier_warehouse_ids: Vec<i32>,
    quantities: Vec<i32>,
    op_index: u64,
) -> Result<(), String> {
    require_positive("warehouse_id", warehouse_id)?;
    require_positive("district_id", district_id)?;
    require_positive("customer_id", customer_id)?;
    validate_order_lines(&item_ids, &supplier_warehouse_ids, &quantities)?;

    let mut district = ctx
        .db
        .district()
        .pk()
        .find(district_pk(warehouse_id, district_id))
        .ok_or_else(|| format!("district not found: w={warehouse_id} d={district_id}"))?;
    let next_order_id = district.d_next_o_id;

    let mut customer = ctx
        .db
        .customer()
        .pk()
        .find(customer_pk(warehouse_id, district_id, customer_id))
        .ok_or_else(|| {
            format!("customer not found: w={warehouse_id} d={district_id} c={customer_id}")
        })?;

    district.d_next_o_id = next_order_id + 1;
    ctx.db.district().pk().update(district);

    let all_local = supplier_warehouse_ids
        .iter()
        .all(|&supplier_warehouse_id| supplier_warehouse_id == warehouse_id);
    let entry_d = format!("stdb-op{op_index}-o{next_order_id}");

    ctx.db.orders().insert(Orders {
        pk: order_pk(warehouse_id, district_id, next_order_id),
        o_id: next_order_id,
        o_d_id: district_id,
        o_w_id: warehouse_id,
        o_c_id: customer_id,
        o_entry_d: entry_d,
        o_carrier_id: 0,
        o_ol_cnt: item_ids.len() as i32,
        o_all_local: if all_local { 1 } else { 0 },
        o_status: "NEW".to_string(),
    });
    ctx.db.new_order().insert(NewOrder {
        pk: order_pk(warehouse_id, district_id, next_order_id),
        no_wd: district_pk(warehouse_id, district_id),
        no_o_id: next_order_id,
        no_d_id: district_id,
        no_w_id: warehouse_id,
    });

    for (idx, ((&item_id, &supplier_warehouse_id), &quantity)) in item_ids
        .iter()
        .zip(supplier_warehouse_ids.iter())
        .zip(quantities.iter())
        .enumerate()
    {
        let item = ctx
            .db
            .item()
            .i_id()
            .find(item_id)
            .ok_or_else(|| format!("item not found: i={item_id}"))?;
        let item_price = item.i_price;
        let mut stock = ctx
            .db
            .stock()
            .pk()
            .find(stock_pk(supplier_warehouse_id, item_id))
            .ok_or_else(|| {
                format!("stock not found: w={supplier_warehouse_id} i={item_id}")
            })?;
        let is_remote = supplier_warehouse_id != warehouse_id;
        let next_stock_ytd = stock.s_ytd + quantity;
        let next_stock_order_cnt = stock.s_order_cnt + 1;
        let next_stock_remote_cnt = stock.s_remote_cnt + if is_remote { 1 } else { 0 };
        let adjusted_quantity = if stock.s_quantity >= quantity + 10 {
            stock.s_quantity - quantity
        } else {
            stock.s_quantity + 91 - quantity
        };
        let amount = item_price * quantity;

        stock.s_quantity = adjusted_quantity;
        stock.s_ytd = next_stock_ytd;
        stock.s_order_cnt = next_stock_order_cnt;
        stock.s_remote_cnt = next_stock_remote_cnt;
        ctx.db.stock().pk().update(stock);

        ctx.db.order_line().insert(OrderLine {
            pk: order_line_pk(warehouse_id, district_id, next_order_id, idx as i32 + 1),
            ol_o_id: next_order_id,
            ol_d_id: district_id,
            ol_w_id: warehouse_id,
            ol_number: idx as i32 + 1,
            ol_i_id: item_id,
            ol_supply_w_id: supplier_warehouse_id,
            ol_delivery_d: String::new(),
            ol_quantity: quantity,
            ol_amount: amount,
        });
    }
    customer.c_last_order_id = next_order_id;
    ctx.db.customer().pk().update(customer);

    Ok(())
}

#[reducer]
pub fn tpcc_payment(
    ctx: &ReducerContext,
    warehouse_id: i32,
    district_id: i32,
    customer_id: i32,
    amount: i32,
    op_index: u64,
) -> Result<(), String> {
    require_positive("warehouse_id", warehouse_id)?;
    require_positive("district_id", district_id)?;
    require_positive("customer_id", customer_id)?;
    require_positive("amount", amount)?;

    let mut warehouse = ctx
        .db
        .warehouse()
        .w_id()
        .find(warehouse_id)
        .ok_or_else(|| format!("warehouse not found: w={warehouse_id}"))?;
    let mut district = ctx
        .db
        .district()
        .pk()
        .find(district_pk(warehouse_id, district_id))
        .ok_or_else(|| format!("district not found: w={warehouse_id} d={district_id}"))?;
    let mut customer = ctx
        .db
        .customer()
        .pk()
        .find(customer_pk(warehouse_id, district_id, customer_id))
        .ok_or_else(|| {
            format!("customer not found: w={warehouse_id} d={district_id} c={customer_id}")
        })?;

    warehouse.w_ytd += amount;
    district.d_ytd += amount;
    customer.c_balance -= amount;
    customer.c_ytd_payment += amount;
    customer.c_payment_cnt += 1;

    ctx.db.warehouse().w_id().update(warehouse);
    ctx.db.district().pk().update(district);
    ctx.db.customer().pk().update(customer);
    ctx.db.history().insert(History {
        h_seq: 0,
        h_id: format!("payment-{warehouse_id}-{district_id}-{customer_id}-{op_index}"),
        h_c_id: customer_id,
        h_c_d_id: district_id,
        h_c_w_id: warehouse_id,
        h_d_id: district_id,
        h_w_id: warehouse_id,
        h_amount: amount,
        h_data: format!("payment warehouse={warehouse_id} district={district_id}"),
    });
    Ok(())
}

#[reducer]
pub fn tpcc_order_status(
    ctx: &ReducerContext,
    warehouse_id: i32,
    district_id: i32,
    customer_id: i32,
    op_index: u64,
) -> Result<(), String> {
    let _ = op_index;
    require_positive("warehouse_id", warehouse_id)?;
    require_positive("district_id", district_id)?;
    require_positive("customer_id", customer_id)?;
    let customer = ctx
        .db
        .customer()
        .pk()
        .find(customer_pk(warehouse_id, district_id, customer_id))
        .ok_or_else(|| {
            format!("customer not found: w={warehouse_id} d={district_id} c={customer_id}")
        })?;
    let order_id = customer.c_last_order_id;
    ctx.db
        .orders()
        .pk()
        .find(order_pk(warehouse_id, district_id, order_id))
        .ok_or_else(|| format!("order not found: w={warehouse_id} d={district_id} o={order_id}"))?;
    Ok(())
}

#[reducer]
pub fn tpcc_delivery(
    ctx: &ReducerContext,
    warehouse_id: i32,
    district_id: i32,
    carrier_id: i32,
    op_index: u64,
) -> Result<(), String> {
    let _ = op_index;
    require_positive("warehouse_id", warehouse_id)?;
    require_positive("district_id", district_id)?;
    require_positive("carrier_id", carrier_id)?;

    let wd = district_pk(warehouse_id, district_id);
    // `pk` encodes no_o_id in its low bits, so the minimum pk within this
    // district is the oldest pending order.
    let new_order_row = ctx
        .db
        .new_order()
        .no_wd()
        .filter(wd)
        .min_by_key(|row| row.pk)
        .ok_or_else(|| "delivery found no pending new_order rows".to_string())?;
    let order_id = new_order_row.no_o_id;
    ctx.db.new_order().pk().delete(new_order_row.pk);

    let mut district = ctx
        .db
        .district()
        .pk()
        .find(wd)
        .ok_or_else(|| format!("district not found: w={warehouse_id} d={district_id}"))?;
    district.d_last_delivery_o_id = order_id;
    ctx.db.district().pk().update(district);

    let mut order = ctx
        .db
        .orders()
        .pk()
        .find(order_pk(warehouse_id, district_id, order_id))
        .ok_or_else(|| {
            format!("order not found: w={warehouse_id} d={district_id} o={order_id}")
        })?;
    let customer_id = order.o_c_id;
    order.o_carrier_id = carrier_id;
    order.o_status = "DELIVERED".to_string();
    ctx.db.orders().pk().update(order);

    let mut customer = ctx
        .db
        .customer()
        .pk()
        .find(customer_pk(warehouse_id, district_id, customer_id))
        .ok_or_else(|| {
            format!("customer not found: w={warehouse_id} d={district_id} c={customer_id}")
        })?;
    customer.c_delivery_cnt += 1;
    ctx.db.customer().pk().update(customer);
    Ok(())
}

#[reducer]
pub fn tpcc_stock_level(
    ctx: &ReducerContext,
    warehouse_id: i32,
    district_id: i32,
    threshold: i32,
    op_index: u64,
) -> Result<(), String> {
    let _ = (district_id, op_index);
    require_positive("warehouse_id", warehouse_id)?;
    require_positive("district_id", district_id)?;
    require_positive("threshold", threshold)?;
    ctx.db
        .stock()
        .s_w_id()
        .filter(warehouse_id)
        .filter(|row| row.s_quantity < threshold)
        .count();
    Ok(())
}

// ---------------------------------------------------------------------------
// Seckill (flash-sale) write-heavy workload.
//
// Mirrors example/tpcc/src/rust/seckill.rs: one hot-row UPDATE plus one
// order INSERT (with payload) per transaction, no reads beyond the stock row.
// ---------------------------------------------------------------------------

#[table(name = seckill_item, public)]
pub struct SeckillItem {
    #[primary_key]
    si_id: i32,
    si_name: String,
    si_stock: i32,
    si_sold: i32,
    si_price: i32,
}

#[table(name = seckill_order, public)]
pub struct SeckillOrder {
    /// Encoded (so_item_id, so_id) primary key.
    #[primary_key]
    pk: u64,
    so_item_id: i32,
    so_id: i32,
    so_user_id: i32,
    so_amount: i32,
    so_payload: String,
}

fn seckill_order_pk(item_id: i32, order_id: i32) -> u64 {
    item_id as u64 * 1_000_000_000_000 + order_id as u64
}

#[reducer]
pub fn seckill_seed(
    ctx: &ReducerContext,
    item_count: i32,
    initial_stock: i32,
    op_index: u64,
) -> Result<(), String> {
    let _ = op_index;
    require_positive("item_count", item_count)?;
    require_positive("initial_stock", initial_stock)?;
    for item_id in 1..=item_count {
        ctx.db.seckill_item().insert(SeckillItem {
            si_id: item_id,
            si_name: format!("promo-item-{item_id}"),
            si_stock: initial_stock,
            si_sold: 0,
            si_price: 100,
        });
    }
    Ok(())
}

/// Sold-out items are a committed no-op (the initial stock is large enough
/// that this never happens during a run).
#[reducer]
pub fn seckill_buy(
    ctx: &ReducerContext,
    item_id: i32,
    order_id: i32,
    user_id: i32,
    amount: i32,
    payload: String,
    op_index: u64,
) -> Result<(), String> {
    let _ = op_index;
    require_positive("item_id", item_id)?;
    require_positive("order_id", order_id)?;
    require_positive("user_id", user_id)?;
    require_positive("amount", amount)?;

    let mut item = ctx
        .db
        .seckill_item()
        .si_id()
        .find(item_id)
        .ok_or_else(|| format!("seckill item {item_id} not found"))?;
    if item.si_stock <= 0 {
        return Ok(());
    }
    item.si_stock -= 1;
    item.si_sold += 1;
    ctx.db.seckill_item().si_id().update(item);
    ctx.db.seckill_order().insert(SeckillOrder {
        pk: seckill_order_pk(item_id, order_id),
        so_item_id: item_id,
        so_id: order_id,
        so_user_id: user_id,
        so_amount: amount,
        so_payload: payload,
    });
    Ok(())
}

// ---------------------------------------------------------------------------
// Hot-row contention injector: K hotspot rows per warehouse; the benchmark
// issues one `tpcc_hotspot_hit` after each TPC-C op (K configurable).
// ---------------------------------------------------------------------------

#[table(name = hotspot, public)]
pub struct Hotspot {
    /// Encoded (h_w_id, h_id) primary key.
    #[primary_key]
    pk: u64,
    h_w_id: i32,
    h_id: i32,
    h_counter: i32,
}

fn hotspot_pk(warehouse_id: i32, hot_id: i32) -> u64 {
    warehouse_id as u64 * 1_000_000 + hot_id as u64
}

#[reducer]
pub fn hotspot_seed(
    ctx: &ReducerContext,
    warehouse_id: i32,
    hot_rows: i32,
    op_index: u64,
) -> Result<(), String> {
    let _ = op_index;
    require_positive("warehouse_id", warehouse_id)?;
    require_positive("hot_rows", hot_rows)?;
    for hot_id in 1..=hot_rows {
        ctx.db.hotspot().insert(Hotspot {
            pk: hotspot_pk(warehouse_id, hot_id),
            h_w_id: warehouse_id,
            h_id: hot_id,
            h_counter: 0,
        });
    }
    Ok(())
}

#[reducer]
pub fn tpcc_hotspot_hit(
    ctx: &ReducerContext,
    warehouse_id: i32,
    hot_id: i32,
    op_index: u64,
) -> Result<(), String> {
    let _ = op_index;
    require_positive("warehouse_id", warehouse_id)?;
    require_positive("hot_id", hot_id)?;
    let mut row = ctx
        .db
        .hotspot()
        .pk()
        .find(hotspot_pk(warehouse_id, hot_id))
        .ok_or_else(|| format!("hotspot row {warehouse_id}/{hot_id} not found"))?;
    row.h_counter += 1;
    ctx.db.hotspot().pk().update(row);
    Ok(())
}
