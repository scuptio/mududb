//! Unit tests for the `rust::orders::Orders` object.

use mududb::common::result::RS;
use mududb::contract::database::field_change::FieldChange;
use mududb::contract::database::object::Object;
use mududb::types::datum::{Datum, DatumDyn};

use crate::rust::orders::object::{Orders, OrdersChange, columns};

fn sample_orders() -> Orders {
    Orders::new(1, Some(2), Some(3), Some(100), Some(1700000000))
}

fn assert_same_orders(lhs: &Orders, rhs: &Orders) {
    assert_eq!(lhs.order_id, rhs.order_id);
    assert_eq!(lhs.user_id, rhs.user_id);
    assert_eq!(lhs.merch_id, rhs.merch_id);
    assert_eq!(lhs.amount, rhs.amount);
    assert_eq!(lhs.created_at, rhs.created_at);
}

/// Unwraps the `update_by_pk` result, failing the test on an empty change.
fn expect_update(change: &OrdersChange, pk: i32) -> RS<(String, Vec<Box<dyn DatumDyn>>)> {
    match change.update_by_pk(pk) {
        Some(parts) => Ok(parts),
        None => Err(mududb::mudu_error!(
            mududb::error::ErrorCode::InvalidState,
            "empty update"
        )),
    }
}

#[test]
fn orders_lifecycle() -> RS<()> {
    let object = sample_orders();
    assert_eq!(object.order_id, 1);
    assert_eq!(object.amount, Some(100));

    // Object metadata
    assert_eq!(Orders::class_name(), "orders");
    assert_eq!(Orders::TABLE_NAME, "orders");
    assert_eq!(Orders::tuple_desc().fields().len(), 5);
    assert_eq!(Orders::tuple_desc().fields()[0].name(), columns::ORDER_ID);
    assert!(Orders::SQL_GET_BY_PK.contains(columns::ORDER_ID));
    assert!(Orders::SQL_INSERT.contains("INSERT INTO orders"));

    // Datum / DatumDyn metadata
    let data_type = Orders::data_type();
    assert_eq!(object.type_family()?, data_type.type_family());

    // Tuple roundtrip
    let tuple = object.to_tuple()?;
    let from_tuple = Orders::from_tuple(&tuple)?;
    assert_same_orders(&from_tuple, &object);

    // Value roundtrip
    let value = object.to_value(&data_type)?;
    let from_value = Orders::from_value(&value)?;
    assert_same_orders(&from_value, &object);

    // Binary roundtrip
    let binary = object.to_binary(&data_type)?;
    let from_binary = Orders::from_binary(binary.as_ref())?;
    assert_same_orders(&from_binary, &object);

    // Textual roundtrip
    let textual = object.to_textual(&data_type)?;
    let from_textual = Orders::from_textual(textual.as_str())?;
    assert_same_orders(&from_textual, &object);

    // Clone through DatumDyn
    let cloned: Box<dyn DatumDyn> = object.clone_boxed();
    let cloned_value = cloned.to_value(&data_type)?;
    let from_cloned = Orders::from_value(&cloned_value)?;
    assert_same_orders(&from_cloned, &object);

    // Insert parameters follow the column declaration order
    let params = object.insert_params();
    assert_eq!(params.0, 1);
    assert_eq!(params.1, Some(2));
    assert_eq!(params.3, Some(100));

    // Partial update statement
    let update = OrdersChange {
        amount: FieldChange::Set(Some(200)),
        created_at: FieldChange::Set(Some(1700000001)),
        ..OrdersChange::default()
    };
    let (sql, update_params) = expect_update(&update, 1)?;
    assert_eq!(
        sql,
        "UPDATE orders SET amount = ?, created_at = ? WHERE order_id = ?"
    );
    assert_eq!(update_params.len(), 3);
    assert!(OrdersChange::default().is_empty());
    assert!(OrdersChange::default().update_by_pk(1).is_none());

    // Set(None) on a nullable column emits `<col> = ?` with a NULL parameter
    let null_update = OrdersChange {
        merch_id: FieldChange::Set(None),
        ..OrdersChange::default()
    };
    let (null_sql, null_params) = expect_update(&null_update, 1)?;
    assert_eq!(
        null_sql,
        "UPDATE orders SET merch_id = ? WHERE order_id = ?"
    );
    assert_eq!(null_params.len(), 2);
    assert!(null_params[0].to_value(&i32::data_type())?.is_null());

    Ok(())
}

#[test]
fn orders_nullable_fields_roundtrip() -> RS<()> {
    let object = Orders::new(2, None, None, None, None);
    let tuple = object.to_tuple()?;
    assert!(tuple.is_null(1));
    assert!(tuple.is_null(4));
    let restored = Orders::from_tuple(&tuple)?;
    assert_same_orders(&restored, &object);

    let value = object.to_tuple_value()?;
    assert!(value.values()[1].is_null());
    let restored = Orders::from_tuple_value(&value)?;
    assert_same_orders(&restored, &object);
    Ok(())
}
