//! Unit tests for the `rust::orders::Orders` object.

use mududb::contract::database::field_change::FieldChange;
use mududb::contract::database::object::Object;
use mududb::types::datum::{Datum, DatumDyn};

use crate::rust::orders::object::{Orders, OrdersChange, columns};

fn sample_orders() -> Orders {
    Orders::new(
        1,
        2,
        3,
        4,
        "entry_val".to_string(),
        Some(5),
        10,
        1,
        "status_val".to_string(),
    )
}

fn assert_same_orders(lhs: &Orders, rhs: &Orders) {
    assert_eq!(lhs.o_id, rhs.o_id);
    assert_eq!(lhs.o_d_id, rhs.o_d_id);
    assert_eq!(lhs.o_w_id, rhs.o_w_id);
    assert_eq!(lhs.o_c_id, rhs.o_c_id);
    assert_eq!(lhs.o_entry_d, rhs.o_entry_d);
    assert_eq!(lhs.o_carrier_id, rhs.o_carrier_id);
    assert_eq!(lhs.o_ol_cnt, rhs.o_ol_cnt);
    assert_eq!(lhs.o_all_local, rhs.o_all_local);
    assert_eq!(lhs.o_status, rhs.o_status);
}

#[test]
fn orders_lifecycle() {
    let object = sample_orders();
    assert_eq!(object.o_id, 1);
    assert_eq!(object.o_ol_cnt, 10);

    // Object metadata
    assert_eq!(Orders::class_name(), "orders");
    assert_eq!(Orders::TABLE_NAME, "orders");
    assert_eq!(Orders::tuple_desc().fields().len(), 9);
    assert_eq!(Orders::tuple_desc().fields()[0].name(), columns::O_ID);
    assert!(Orders::SQL_GET_BY_PK.contains(columns::O_ID));
    assert!(Orders::SQL_INSERT.contains("INSERT INTO orders"));

    // Datum / DatumDyn metadata
    let data_type = Orders::data_type();
    assert_eq!(object.type_family().unwrap(), data_type.type_family());

    // Tuple / value / binary / textual roundtrips
    let tuple = object.to_tuple().unwrap();
    assert_same_orders(&Orders::from_tuple(&tuple).unwrap(), &object);
    let value = object.to_value(&data_type).unwrap();
    assert_same_orders(&Orders::from_value(&value).unwrap(), &object);
    let binary = object.to_binary(&data_type).unwrap();
    assert_same_orders(&Orders::from_binary(binary.as_ref()).unwrap(), &object);
    let textual = object.to_textual(&data_type).unwrap();
    assert_same_orders(&Orders::from_textual(textual.as_str()).unwrap(), &object);

    // Clone through DatumDyn
    let cloned: Box<dyn DatumDyn> = object.clone_boxed();
    let cloned_value = cloned.to_value(&data_type).unwrap();
    assert_same_orders(&Orders::from_value(&cloned_value).unwrap(), &object);

    // Insert parameters follow the column declaration order
    let params = object.insert_params();
    assert_eq!(params.0, 1);
    assert_eq!(params.4, "entry_val");
    assert_eq!(params.5, Some(5));

    // Partial update statement over the composite primary key
    let update = OrdersChange {
        o_carrier_id: FieldChange::Set(Some(6)),
        o_status: FieldChange::Set("shipped".to_string()),
        ..OrdersChange::default()
    };
    let (sql, update_params) = update.update_by_pk(1, 2, 3).unwrap();
    assert_eq!(
        sql,
        "UPDATE orders SET o_carrier_id = ?, o_status = ? WHERE o_id = ? AND o_d_id = ? AND o_w_id = ?"
    );
    assert_eq!(update_params.len(), 5);
    assert!(OrdersChange::default().is_empty());
    assert!(OrdersChange::default().update_by_pk(1, 2, 3).is_none());

    // Set(None) on a nullable column emits `<col> = ?` with a NULL parameter
    let null_update = OrdersChange {
        o_carrier_id: FieldChange::Set(None),
        ..OrdersChange::default()
    };
    let (null_sql, null_params) = null_update.update_by_pk(1, 2, 3).unwrap();
    assert_eq!(
        null_sql,
        "UPDATE orders SET o_carrier_id = ? WHERE o_id = ? AND o_d_id = ? AND o_w_id = ?"
    );
    assert_eq!(null_params.len(), 4);
    assert!(
        null_params[0]
            .to_value(&i32::data_type())
            .unwrap()
            .is_null()
    );
}

#[test]
fn orders_nullable_fields_roundtrip() {
    let object = Orders::new(
        1,
        2,
        3,
        4,
        "entry_val".to_string(),
        None,
        10,
        1,
        "s".to_string(),
    );
    let tuple = object.to_tuple().unwrap();
    assert!(tuple.is_null(5));
    let restored = Orders::from_tuple(&tuple).unwrap();
    assert_same_orders(&restored, &object);

    let value = object.to_tuple_value().unwrap();
    assert!(value.values()[5].is_null());
    let restored = Orders::from_tuple_value(&value).unwrap();
    assert_same_orders(&restored, &object);
}
