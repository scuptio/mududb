//! Relation-level point access for the TPC-C procedures.
//!
//! These helpers address one relation row by primary key through the
//! `relation_get` / `relation_update` / `relation_insert` syscalls, bypassing
//! SQL parsing and result-set serialization. Attribute indices are the catalog
//! column positions assigned by `bind_create_table`: primary-key columns first
//! (in primary-key order), then the value columns in `CREATE TABLE` definition
//! order (see `sql/ddl.sql` and `sql/ddl_warehouse_partitioned.sql`); datums
//! are encoded in the column's binary format.
//!
//! Not every driver implements the relation syscalls (only the wasm host and
//! the sqlite standalone driver do). When a syscall fails with
//! `ErrorCode::NotImplemented`, the helpers fall back to the equivalent
//! parameterized SQL (`mudu_query` / `mudu_command`); the detection is cached
//! process-wide so the fallback costs one failed syscall, not one per call.
#![allow(missing_docs)]

use mududb::binding::universal::uni_relation::{
    RELATION_DELTA_OP_ADD, RELATION_DELTA_OP_ADD_DEFERRED, RELATION_DELTA_OP_SUB,
    RELATION_DELTA_OP_SUB_DEFERRED, RELATION_DELTA_OP_SUB_WRAP_DEFERRED, UniRelationDelta,
};
use mududb::common::id::OID;
use mududb::common::result::RS;
use mududb::contract::database::sql_param_value::SQLParamValue;
use mududb::contract::{sql_params, sql_stmt};
use mududb::error::{ErrorCode, MuduError};
use mududb::mudu::data_type::numeric::Numeric;
use mududb::mudu_error;
use mududb::sys_interface::sync_api::{
    mudu_command, mudu_query, mudu_relation_get, mudu_relation_insert, mudu_relation_update,
};
use mududb::types::data_type::DataType;
use mududb::types::data_type_function::{recv_binary, send_binary};
use mududb::types::data_type_param_kind::DataTypeParamKind;
use mududb::types::data_type_param_numeric::DataTypeParamNumeric;
use mududb::types::data_value::DataValue;
use mududb::types::type_family::TypeFamily;
use std::sync::atomic::{AtomicU8, Ordering};

// table names
pub const TABLE_WAREHOUSE: &str = "warehouse";
pub const TABLE_DISTRICT: &str = "district";
pub const TABLE_CUSTOMER: &str = "customer";
pub const TABLE_ITEM: &str = "item";
pub const TABLE_STOCK: &str = "stock";
pub const TABLE_ORDERS: &str = "orders";
pub const TABLE_NEW_ORDER: &str = "new_order";
pub const TABLE_ORDER_LINE: &str = "order_line";
pub const TABLE_HISTORY: &str = "history";

// warehouse columns
pub const W_ID: u64 = 0;
pub const W_YTD: u64 = 3;

// district columns (key columns d_w_id, d_id first)
pub const D_W_ID: u64 = 0;
pub const D_ID: u64 = 1;
pub const D_TAX: u64 = 3;
pub const D_YTD: u64 = 4;
pub const D_NEXT_O_ID: u64 = 5;

// customer columns (key columns c_w_id, c_d_id, c_id first)
pub const C_W_ID: u64 = 0;
pub const C_D_ID: u64 = 1;
pub const C_ID: u64 = 2;
pub const C_BALANCE: u64 = 7;
pub const C_YTD_PAYMENT: u64 = 8;
pub const C_PAYMENT_CNT: u64 = 9;
pub const C_LAST_ORDER_ID: u64 = 11;

// item columns (unpartitioned: i_id is the only key column)
pub const I_ID: u64 = 0;
pub const I_PRICE: u64 = 2;

// item columns (warehouse-partitioned: i_w_id prepended)
pub const PI_W_ID: u64 = 0;
pub const PI_ID: u64 = 1;
pub const PI_PRICE: u64 = 3;

// stock columns (key columns s_w_id, s_i_id first)
pub const S_W_ID: u64 = 0;
pub const S_I_ID: u64 = 1;
pub const S_QUANTITY: u64 = 2;
pub const S_YTD: u64 = 3;
pub const S_ORDER_CNT: u64 = 4;
pub const S_REMOTE_CNT: u64 = 5;

// orders columns (key columns o_w_id, o_d_id, o_id first)
pub const O_W_ID: u64 = 0;
pub const O_D_ID: u64 = 1;
pub const O_ID: u64 = 2;
pub const O_C_ID: u64 = 3;
pub const O_ENTRY_D: u64 = 4;
pub const O_CARRIER_ID: u64 = 5;
pub const O_OL_CNT: u64 = 6;
pub const O_ALL_LOCAL: u64 = 7;
pub const O_STATUS: u64 = 8;

// new_order columns (key columns no_w_id, no_d_id, no_o_id first)
pub const NO_W_ID: u64 = 0;
pub const NO_D_ID: u64 = 1;
pub const NO_O_ID: u64 = 2;

// order_line columns (key columns ol_w_id, ol_d_id, ol_o_id, ol_number first)
pub const OL_W_ID: u64 = 0;
pub const OL_D_ID: u64 = 1;
pub const OL_O_ID: u64 = 2;
pub const OL_NUMBER: u64 = 3;
pub const OL_I_ID: u64 = 4;
pub const OL_SUPPLY_W_ID: u64 = 5;
pub const OL_DELIVERY_D: u64 = 6;
pub const OL_QUANTITY: u64 = 7;
pub const OL_AMOUNT: u64 = 8;

// history columns (unpartitioned: h_id is the only key column)
pub const H_ID: u64 = 0;
pub const H_C_ID: u64 = 1;
pub const H_C_D_ID: u64 = 2;
pub const H_C_W_ID: u64 = 3;
pub const H_D_ID: u64 = 4;
pub const H_W_ID: u64 = 5;
pub const H_AMOUNT: u64 = 6;
pub const H_DATA: u64 = 7;

// history columns (warehouse-partitioned: h_w_id prepended)
pub const PH_W_ID: u64 = 0;
pub const PH_ID: u64 = 1;
pub const PH_C_ID: u64 = 2;
pub const PH_C_D_ID: u64 = 3;
pub const PH_C_W_ID: u64 = 4;
pub const PH_D_ID: u64 = 5;
pub const PH_AMOUNT: u64 = 6;
pub const PH_DATA: u64 = 7;

/// `NUMERIC(12,2)` money columns (w_ytd, d_ytd, c_balance, c_ytd_payment).
pub const MONEY_12_2: (u8, u8) = (12, 2);
/// `NUMERIC(6,2)` money columns (i_price, h_amount).
pub const MONEY_6_2: (u8, u8) = (6, 2);
/// `NUMERIC(8,2)` money columns (ol_amount).
pub const MONEY_8_2: (u8, u8) = (8, 2);

fn money_type((precision, scale): (u8, u8)) -> DataType {
    DataType::from_id_param(
        TypeFamily::Numeric,
        Some(DataTypeParamKind::Numeric(Box::new(
            DataTypeParamNumeric::new(precision, scale),
        ))),
    )
}

/// Column kind needed to decode/encode relation datums as SQL values.
#[derive(Clone, Copy)]
enum ColumnKind {
    Int,
    Text,
    Money((u8, u8)),
}

/// One column of a TPC-C table layout used by the SQL fallback.
struct Column {
    name: &'static str,
    kind: ColumnKind,
}

const fn col(name: &'static str, kind: ColumnKind) -> Column {
    Column { name, kind }
}

// Column layouts in catalog order (attribute index = position): primary-key
// columns first in key order, then the value columns in `CREATE TABLE`
// definition order, matching the `bind_create_table` schema layout. `item` and
// `history` have a warehouse-partitioned variant with `i_w_id` / `h_w_id`
// prepended; the fallback picks the variant from the primary-key length (the
// partitioned keys carry one more column), which is unambiguous for these two
// tables.
const WAREHOUSE_COLUMNS: &[Column] = &[
    col("w_id", ColumnKind::Int),
    col("w_name", ColumnKind::Text),
    col("w_tax", ColumnKind::Int),
    col("w_ytd", ColumnKind::Money(MONEY_12_2)),
];
const DISTRICT_COLUMNS: &[Column] = &[
    col("d_w_id", ColumnKind::Int),
    col("d_id", ColumnKind::Int),
    col("d_name", ColumnKind::Text),
    col("d_tax", ColumnKind::Int),
    col("d_ytd", ColumnKind::Money(MONEY_12_2)),
    col("d_next_o_id", ColumnKind::Int),
    col("d_last_delivery_o_id", ColumnKind::Int),
];
const CUSTOMER_COLUMNS: &[Column] = &[
    col("c_w_id", ColumnKind::Int),
    col("c_d_id", ColumnKind::Int),
    col("c_id", ColumnKind::Int),
    col("c_first", ColumnKind::Text),
    col("c_last", ColumnKind::Text),
    col("c_discount", ColumnKind::Int),
    col("c_credit", ColumnKind::Text),
    col("c_balance", ColumnKind::Money(MONEY_12_2)),
    col("c_ytd_payment", ColumnKind::Money(MONEY_12_2)),
    col("c_payment_cnt", ColumnKind::Int),
    col("c_delivery_cnt", ColumnKind::Int),
    col("c_last_order_id", ColumnKind::Int),
];
const ITEM_COLUMNS: &[Column] = &[
    col("i_id", ColumnKind::Int),
    col("i_name", ColumnKind::Text),
    col("i_price", ColumnKind::Money(MONEY_6_2)),
];
const ITEM_PARTITIONED_COLUMNS: &[Column] = &[
    col("i_w_id", ColumnKind::Int),
    col("i_id", ColumnKind::Int),
    col("i_name", ColumnKind::Text),
    col("i_price", ColumnKind::Money(MONEY_6_2)),
];
const STOCK_COLUMNS: &[Column] = &[
    col("s_w_id", ColumnKind::Int),
    col("s_i_id", ColumnKind::Int),
    col("s_quantity", ColumnKind::Int),
    col("s_ytd", ColumnKind::Int),
    col("s_order_cnt", ColumnKind::Int),
    col("s_remote_cnt", ColumnKind::Int),
];
const ORDERS_COLUMNS: &[Column] = &[
    col("o_w_id", ColumnKind::Int),
    col("o_d_id", ColumnKind::Int),
    col("o_id", ColumnKind::Int),
    col("o_c_id", ColumnKind::Int),
    col("o_entry_d", ColumnKind::Text),
    col("o_carrier_id", ColumnKind::Int),
    col("o_ol_cnt", ColumnKind::Int),
    col("o_all_local", ColumnKind::Int),
    col("o_status", ColumnKind::Text),
];
const NEW_ORDER_COLUMNS: &[Column] = &[
    col("no_w_id", ColumnKind::Int),
    col("no_d_id", ColumnKind::Int),
    col("no_o_id", ColumnKind::Int),
];
const ORDER_LINE_COLUMNS: &[Column] = &[
    col("ol_w_id", ColumnKind::Int),
    col("ol_d_id", ColumnKind::Int),
    col("ol_o_id", ColumnKind::Int),
    col("ol_number", ColumnKind::Int),
    col("ol_i_id", ColumnKind::Int),
    col("ol_supply_w_id", ColumnKind::Int),
    col("ol_delivery_d", ColumnKind::Text),
    col("ol_quantity", ColumnKind::Int),
    col("ol_amount", ColumnKind::Money(MONEY_8_2)),
];
const HISTORY_COLUMNS: &[Column] = &[
    col("h_id", ColumnKind::Text),
    col("h_c_id", ColumnKind::Int),
    col("h_c_d_id", ColumnKind::Int),
    col("h_c_w_id", ColumnKind::Int),
    col("h_d_id", ColumnKind::Int),
    col("h_w_id", ColumnKind::Int),
    col("h_amount", ColumnKind::Money(MONEY_6_2)),
    col("h_data", ColumnKind::Text),
];
const HISTORY_PARTITIONED_COLUMNS: &[Column] = &[
    col("h_w_id", ColumnKind::Int),
    col("h_id", ColumnKind::Text),
    col("h_c_id", ColumnKind::Int),
    col("h_c_d_id", ColumnKind::Int),
    col("h_c_w_id", ColumnKind::Int),
    col("h_d_id", ColumnKind::Int),
    col("h_amount", ColumnKind::Money(MONEY_6_2)),
    col("h_data", ColumnKind::Text),
];

/// Resolves the column layout of `table`; `key_len` disambiguates the
/// partitioned variants of `item` and `history` (see the layout comment).
fn table_columns(table: &str, key_len: usize) -> RS<&'static [Column]> {
    match table {
        TABLE_WAREHOUSE => Ok(WAREHOUSE_COLUMNS),
        TABLE_DISTRICT => Ok(DISTRICT_COLUMNS),
        TABLE_CUSTOMER => Ok(CUSTOMER_COLUMNS),
        TABLE_ITEM => Ok(if key_len <= 1 {
            ITEM_COLUMNS
        } else {
            ITEM_PARTITIONED_COLUMNS
        }),
        TABLE_STOCK => Ok(STOCK_COLUMNS),
        TABLE_ORDERS => Ok(ORDERS_COLUMNS),
        TABLE_NEW_ORDER => Ok(NEW_ORDER_COLUMNS),
        TABLE_ORDER_LINE => Ok(ORDER_LINE_COLUMNS),
        TABLE_HISTORY => Ok(if key_len <= 1 {
            HISTORY_COLUMNS
        } else {
            HISTORY_PARTITIONED_COLUMNS
        }),
        _ => Err(mudu_error!(
            ErrorCode::InvalidArgument,
            format!("relation SQL fallback: unknown table {table}")
        )),
    }
}

fn column_at<'a>(columns: &'a [Column], table: &str, attr: u64) -> RS<&'a Column> {
    columns.get(attr as usize).ok_or_else(|| {
        mudu_error!(
            ErrorCode::InvalidArgument,
            format!("relation SQL fallback: {table} has no column {attr}")
        )
    })
}

fn column_data_type(kind: ColumnKind) -> DataType {
    match kind {
        ColumnKind::Int => DataType::default_for(TypeFamily::I32),
        ColumnKind::Text => DataType::default_for(TypeFamily::String),
        ColumnKind::Money(money) => money_type(money),
    }
}

/// Decodes a relation datum into the SQL parameter value of the column.
fn decode_datum(datum: &[u8], kind: ColumnKind) -> RS<DataValue> {
    recv_binary(datum, &column_data_type(kind))
        .map(|value| (*value).clone())
        .map_err(|e| e.to_m_err())
}

/// Encodes a query result value back into the relation datum format.
fn encode_datum(value: DataValue, kind: ColumnKind) -> RS<Vec<u8>> {
    send_binary(&value, &column_data_type(kind)).map_err(|e| e.to_m_err())
}

/// Builds the `col = ? AND ...` key predicate and its decoded parameters.
fn sql_key_predicate(
    columns: &[Column],
    table: &str,
    key: &[(u64, Vec<u8>)],
) -> RS<(String, Vec<DataValue>)> {
    if key.is_empty() {
        return Err(mudu_error!(
            ErrorCode::InvalidArgument,
            format!("relation {table} key predicate is empty")
        ));
    }
    let mut conditions = Vec::with_capacity(key.len());
    let mut values = Vec::with_capacity(key.len());
    for (attr, datum) in key {
        let column = column_at(columns, table, *attr)?;
        conditions.push(format!("{} = ?", column.name));
        values.push(decode_datum(datum, column.kind)?);
    }
    Ok((conditions.join(" AND "), values))
}

/// Not probed yet: use the relation syscalls.
const RELATION_SUPPORT_UNKNOWN: u8 = 0;
/// The driver answers the relation syscalls; use them directly.
const RELATION_SUPPORT_NATIVE: u8 = 1;
/// The driver answered `NotImplemented`; use the SQL fallback directly.
const RELATION_SUPPORT_SQL: u8 = 2;

/// Process-wide cache of relation-syscall support: the first `NotImplemented`
/// switches every later call to the SQL fallback without another failed
/// round trip.
static RELATION_SUPPORT: AtomicU8 = AtomicU8::new(RELATION_SUPPORT_UNKNOWN);

fn is_not_implemented(error: &MuduError) -> bool {
    error.ec() == ErrorCode::NotImplemented
}

#[cfg(test)]
pub(crate) fn force_relation_sql_fallback_for_test() {
    RELATION_SUPPORT.store(RELATION_SUPPORT_SQL, Ordering::SeqCst);
}

#[cfg(test)]
pub(crate) fn reset_relation_support_for_test() {
    RELATION_SUPPORT.store(RELATION_SUPPORT_UNKNOWN, Ordering::SeqCst);
}

/// Encodes an `INTEGER` column datum.
pub fn datum_i32(value: i32) -> RS<Vec<u8>> {
    send_binary(
        &DataValue::from_i32(value),
        &DataType::default_for(TypeFamily::I32),
    )
    .map_err(|e| e.to_m_err())
}

/// Encodes a `TEXT` column datum.
pub fn datum_text(value: &str) -> RS<Vec<u8>> {
    send_binary(
        &DataValue::from_string(value.to_string()),
        &DataType::default_for(TypeFamily::String),
    )
    .map_err(|e| e.to_m_err())
}

/// Encodes a whole-number money column datum (NUMERIC(precision, scale)).
pub fn datum_money(amount: i32, money: (u8, u8)) -> RS<Vec<u8>> {
    send_binary(
        &DataValue::from_numeric(Numeric::from(amount)),
        &money_type(money),
    )
    .map_err(|e| e.to_m_err())
}

fn require_field<'a>(row: &'a [Option<Vec<u8>>], index: usize, what: &str) -> RS<&'a [u8]> {
    row.get(index)
        .and_then(|field| field.as_deref())
        .ok_or_else(|| {
            mudu_error!(
                ErrorCode::InvalidState,
                format!("relation field is null: {what}")
            )
        })
}

/// Decodes an `INTEGER` column datum read from a relation row.
pub fn read_i32(row: &[Option<Vec<u8>>], index: usize, what: &str) -> RS<i32> {
    let bytes = require_field(row, index, what)?;
    let value =
        recv_binary(bytes, &DataType::default_for(TypeFamily::I32)).map_err(|e| e.to_m_err())?;
    Ok(*value.expect_i32())
}

/// Decodes a whole-number money column datum (the value must be integral;
/// see `required_money_i32` in the procedure module for the rationale).
pub fn read_money(
    row: &[Option<Vec<u8>>],
    index: usize,
    money: (u8, u8),
    what: &str,
) -> RS<Numeric> {
    let bytes = require_field(row, index, what)?;
    let value = recv_binary(bytes, &money_type(money)).map_err(|e| e.to_m_err())?;
    Ok(value.expect_numeric().clone())
}

/// Wraps a not-found relation row as an `EntityNotFound` error.
pub fn require_row(row: Option<Vec<Option<Vec<u8>>>>, table: &str) -> RS<Vec<Option<Vec<u8>>>> {
    row.ok_or_else(|| {
        mudu_error!(
            ErrorCode::EntityNotFound,
            format!("relation row not found: {table}")
        )
    })
}

/// Point-read one relation row by primary key.
pub fn rel_get(
    xid: OID,
    table: &str,
    key: &[(u64, Vec<u8>)],
    select: &[u64],
) -> RS<Option<Vec<Option<Vec<u8>>>>> {
    if RELATION_SUPPORT.load(Ordering::Relaxed) == RELATION_SUPPORT_SQL {
        return rel_get_sql(xid, table, key, select);
    }
    match mudu_relation_get(xid, table, key, select) {
        Err(error) if is_not_implemented(&error) => {
            RELATION_SUPPORT.store(RELATION_SUPPORT_SQL, Ordering::Relaxed);
            rel_get_sql(xid, table, key, select)
        }
        result => {
            RELATION_SUPPORT.store(RELATION_SUPPORT_NATIVE, Ordering::Relaxed);
            result
        }
    }
}

/// Point-read one relation row that must exist by primary key.
pub fn rel_get_one(
    xid: OID,
    table: &str,
    key: &[(u64, Vec<u8>)],
    select: &[u64],
) -> RS<Vec<Option<Vec<u8>>>> {
    require_row(rel_get(xid, table, key, select)?, table)
}

/// Read-modify-write one relation row by primary key.
pub fn rel_update(
    xid: OID,
    table: &str,
    key: &[(u64, Vec<u8>)],
    values: &[(u64, Vec<u8>)],
    deltas: &[UniRelationDelta],
) -> RS<u64> {
    if RELATION_SUPPORT.load(Ordering::Relaxed) == RELATION_SUPPORT_SQL {
        return rel_update_sql(xid, table, key, values, deltas);
    }
    match mudu_relation_update(xid, table, key, values, deltas) {
        Err(error) if is_not_implemented(&error) => {
            RELATION_SUPPORT.store(RELATION_SUPPORT_SQL, Ordering::Relaxed);
            rel_update_sql(xid, table, key, values, deltas)
        }
        result => {
            RELATION_SUPPORT.store(RELATION_SUPPORT_NATIVE, Ordering::Relaxed);
            result
        }
    }
}

/// Insert one relation row.
pub fn rel_insert(
    xid: OID,
    table: &str,
    key: &[(u64, Vec<u8>)],
    values: &[(u64, Vec<u8>)],
) -> RS<()> {
    if RELATION_SUPPORT.load(Ordering::Relaxed) == RELATION_SUPPORT_SQL {
        return rel_insert_sql(xid, table, key, values);
    }
    match mudu_relation_insert(xid, table, key, values) {
        Err(error) if is_not_implemented(&error) => {
            RELATION_SUPPORT.store(RELATION_SUPPORT_SQL, Ordering::Relaxed);
            rel_insert_sql(xid, table, key, values)
        }
        result => {
            RELATION_SUPPORT.store(RELATION_SUPPORT_NATIVE, Ordering::Relaxed);
            result
        }
    }
}

/// SQL fallback of [`rel_get`]: `SELECT <col> FROM <table> WHERE <key>`.
///
/// The typed query API decodes a single column per query, so a multi-column
/// projection is read one column at a time; every TPC-C call site projects a
/// single column, keeping this one round trip. An empty projection probes row
/// existence through the first key column. NULL fields are not decodable
/// through the typed query API and surface as an error (all TPC-C projected
/// columns are `NOT NULL`).
fn rel_get_sql(
    xid: OID,
    table: &str,
    key: &[(u64, Vec<u8>)],
    select: &[u64],
) -> RS<Option<Vec<Option<Vec<u8>>>>> {
    let columns = table_columns(table, key.len())?;
    let (predicate, key_values) = sql_key_predicate(columns, table, key)?;
    // An empty projection probes row existence through the first key column.
    let probe_select;
    let probe = select.is_empty();
    let select = if probe {
        probe_select = [key[0].0];
        &probe_select[..]
    } else {
        select
    };
    let mut row = Vec::with_capacity(select.len());
    for (position, attr) in select.iter().enumerate() {
        let column = column_at(columns, table, *attr)?;
        let sql = format!("SELECT {} FROM {table} WHERE {predicate}", column.name);
        let params = SQLParamValue::from_vec(key_values.clone());
        let value: Option<DataValue> = match column.kind {
            ColumnKind::Int => mudu_query::<i32>(xid, sql_stmt!(&sql), sql_params!(&params))?
                .next_record()?
                .map(DataValue::from_i32),
            ColumnKind::Text => mudu_query::<String>(xid, sql_stmt!(&sql), sql_params!(&params))?
                .next_record()?
                .map(DataValue::from_string),
            ColumnKind::Money(_) => {
                mudu_query::<Numeric>(xid, sql_stmt!(&sql), sql_params!(&params))?
                    .next_record()?
                    .map(DataValue::from_numeric)
            }
        };
        if position == 0 && value.is_none() {
            return Ok(None);
        }
        row.push(
            value
                .map(|value| encode_datum(value, column.kind))
                .transpose()?,
        );
    }
    if probe {
        return Ok(Some(Vec::new()));
    }
    Ok(Some(row))
}

/// SQL fallback of [`rel_update`]:
/// `UPDATE <table> SET <col> = ?, <col> = <col> +|- ? ... WHERE <key>`.
///
/// Counter deltas stay engine-side expressions (`col = col +|- ?`), so the
/// atomic read-modify-write semantics of the relation syscall are preserved.
/// Deferred add/sub map onto the same expressions (the fallback path is
/// statement-time locked anyway). A conditional restock delta cannot be one
/// expression here (the SQL dialect has no `CASE` and no non-key predicates),
/// so it is resolved as an in-procedure read-modify-write: the current column
/// value is point-read by primary key inside the same transaction, the
/// restock result `current - q (+ wrap if below floor)` is computed here, and
/// the outcome is merged into the update as an absolute assignment. The read
/// lock taken by the point read is held to commit, keeping the
/// read-modify-write atomic against other transactions.
fn rel_update_sql(
    xid: OID,
    table: &str,
    key: &[(u64, Vec<u8>)],
    values: &[(u64, Vec<u8>)],
    deltas: &[UniRelationDelta],
) -> RS<u64> {
    let columns = table_columns(table, key.len())?;
    let mut assignments = Vec::with_capacity(values.len() + deltas.len());
    let mut params = Vec::with_capacity(values.len() + deltas.len() + key.len());
    for (attr, datum) in values {
        let column = column_at(columns, table, *attr)?;
        assignments.push(format!("{} = ?", column.name));
        params.push(decode_datum(datum, column.kind)?);
    }
    for delta in deltas {
        let column = column_at(columns, table, delta.attr)?;
        match delta.op {
            RELATION_DELTA_OP_ADD | RELATION_DELTA_OP_ADD_DEFERRED => {
                assignments.push(format!("{0} = {0} + ?", column.name));
            }
            RELATION_DELTA_OP_SUB | RELATION_DELTA_OP_SUB_DEFERRED => {
                assignments.push(format!("{0} = {0} - ?", column.name));
            }
            RELATION_DELTA_OP_SUB_WRAP_DEFERRED => {
                let (quantity, floor, wrap) = decode_sub_wrap_datum(&delta.datum)?;
                if !matches!(column.kind, ColumnKind::Int) {
                    return Err(mudu_error!(
                        ErrorCode::InvalidType,
                        format!(
                            "conditional restock requires an integer column, got {}",
                            column.name
                        )
                    ));
                }
                if wrap <= floor {
                    return Err(mudu_error!(
                        ErrorCode::InvalidArgument,
                        format!(
                            "conditional restock requires wrap > floor, got wrap={wrap} floor={floor}"
                        )
                    ));
                }
                // Locked read: the point read runs inside this transaction,
                // so its lock serializes the read-modify-write against other
                // transactions touching the same row.
                let current_row = require_row(rel_get_sql(xid, table, key, &[delta.attr])?, table)?;
                let current_datum = current_row.into_iter().next().ok_or_else(|| {
                    mudu_error!(
                        ErrorCode::InvalidTuple,
                        format!(
                            "conditional restock read of {} returned no column",
                            column.name
                        )
                    )
                })?;
                let current = match current_datum {
                    Some(datum) => *decode_datum(&datum, column.kind)?.expect_i32(),
                    None => {
                        return Err(mudu_error!(
                            ErrorCode::InvalidTuple,
                            format!("conditional restock column {} is NULL", column.name)
                        ));
                    }
                };
                let adjusted = sub_wrap_result(current as i64, quantity, floor, wrap)?;
                assignments.push(format!("{} = ?", column.name));
                params.push(DataValue::from_i32(i32::try_from(adjusted).map_err(
                    |_| {
                        mudu_error!(
                            ErrorCode::InvalidTuple,
                            "conditional restock result overflows i32"
                        )
                    },
                )?));
                continue;
            }
            other => {
                return Err(mudu_error!(
                    ErrorCode::Decode,
                    format!("unknown relation delta op {other}")
                ));
            }
        }
        params.push(decode_datum(&delta.datum, column.kind)?);
    }
    if assignments.is_empty() {
        return Err(mudu_error!(
            ErrorCode::InvalidArgument,
            format!("relation {table} update has no assignments")
        ));
    }
    let (predicate, key_values) = sql_key_predicate(columns, table, key)?;
    params.extend(key_values);
    let sql = format!(
        "UPDATE {table} SET {} WHERE {predicate}",
        assignments.join(", ")
    );
    let affected = mudu_command(
        xid,
        sql_stmt!(&sql),
        sql_params!(&SQLParamValue::from_vec(params)),
    )?;
    Ok(affected)
}

/// Computes the conditional-restock outcome `current - q (+ wrap when the
/// result drops below floor)`, mirroring the kernel's `SubWrapDeferred`
/// evaluation. `wrap > floor` is validated by the caller.
fn sub_wrap_result(current: i64, quantity: i64, floor: i64, wrap: i64) -> RS<i64> {
    let y = current
        .checked_sub(quantity)
        .ok_or_else(|| mudu_error!(ErrorCode::InvalidTuple, "restock delta overflows"))?;
    let result = if y < floor {
        y.checked_add(wrap)
            .ok_or_else(|| mudu_error!(ErrorCode::InvalidTuple, "restock wrap overflows"))?
    } else {
        y
    };
    Ok(result)
}

/// Unpacks a conditional-restock delta datum (`[q, floor, wrap]`, three
/// big-endian i64s, see `UniRelationDelta::sub_wrap`).
fn decode_sub_wrap_datum(datum: &[u8]) -> RS<(i64, i64, i64)> {
    if datum.len() != 24 {
        return Err(mudu_error!(
            ErrorCode::Decode,
            format!(
                "conditional restock delta expects a 24-byte [q, floor, wrap] datum, got {}",
                datum.len()
            )
        ));
    }
    let read =
        |offset: usize| i64::from_be_bytes(datum[offset..offset + 8].try_into().unwrap_or([0; 8]));
    Ok((read(0), read(8), read(16)))
}

/// SQL fallback of [`rel_insert`]:
/// `INSERT INTO <table> (<key cols>, <value cols>) VALUES (?, ...)`; a
/// duplicate primary key fails with the engine's constraint-violation error,
/// matching the pre-relation SQL behavior.
fn rel_insert_sql(
    xid: OID,
    table: &str,
    key: &[(u64, Vec<u8>)],
    values: &[(u64, Vec<u8>)],
) -> RS<()> {
    let columns = table_columns(table, key.len())?;
    let mut names = Vec::with_capacity(key.len() + values.len());
    let mut placeholders = Vec::with_capacity(key.len() + values.len());
    let mut params = Vec::with_capacity(key.len() + values.len());
    for (attr, datum) in key.iter().chain(values.iter()) {
        let column = column_at(columns, table, *attr)?;
        names.push(column.name);
        placeholders.push("?");
        params.push(decode_datum(datum, column.kind)?);
    }
    let sql = format!(
        "INSERT INTO {table} ({}) VALUES ({})",
        names.join(", "),
        placeholders.join(", ")
    );
    mudu_command(
        xid,
        sql_stmt!(&sql),
        sql_params!(&SQLParamValue::from_vec(params)),
    )?;
    Ok(())
}

/// Builds a key predicate list from `(attr, i32)` pairs.
pub fn key_i32(pairs: &[(u64, i32)]) -> RS<Vec<(u64, Vec<u8>)>> {
    pairs
        .iter()
        .map(|(attr, value)| Ok((*attr, datum_i32(*value)?)))
        .collect()
}
