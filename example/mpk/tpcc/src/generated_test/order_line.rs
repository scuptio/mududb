//! Unit tests for the `generated::order_line::OrderLine` object.

use mududb::contract::database::field_change::FieldChange;
use mududb::contract::database::object::Object;
use mududb::mudu::data_type::numeric::Numeric;
use mududb::types::datum::{Datum, DatumDyn};

use crate::generated::order_line::object::{OrderLine, OrderLineChange, columns};

fn sample_order_line() -> OrderLine {
    OrderLine::new(
        1,
        2,
        3,
        4,
        5,
        6,
        Some("delivery_val".to_string()),
        10,
        Numeric::from(100i32),
    )
}

fn assert_same_order_line(lhs: &OrderLine, rhs: &OrderLine) {
    assert_eq!(lhs.ol_o_id, rhs.ol_o_id);
    assert_eq!(lhs.ol_d_id, rhs.ol_d_id);
    assert_eq!(lhs.ol_w_id, rhs.ol_w_id);
    assert_eq!(lhs.ol_number, rhs.ol_number);
    assert_eq!(lhs.ol_i_id, rhs.ol_i_id);
    assert_eq!(lhs.ol_supply_w_id, rhs.ol_supply_w_id);
    assert_eq!(lhs.ol_delivery_d, rhs.ol_delivery_d);
    assert_eq!(lhs.ol_quantity, rhs.ol_quantity);
    assert_eq!(lhs.ol_amount, rhs.ol_amount);
}

#[test]
fn order_line_lifecycle() {
    let object = sample_order_line();
    assert_eq!(object.ol_number, 4);
    assert_eq!(object.ol_quantity, 10);

    // Object metadata
    assert_eq!(OrderLine::class_name(), "order_line");
    assert_eq!(OrderLine::TABLE_NAME, "order_line");
    assert_eq!(OrderLine::tuple_desc().fields().len(), 9);
    assert_eq!(OrderLine::tuple_desc().fields()[0].name(), columns::OL_O_ID);
    assert!(OrderLine::SQL_GET_BY_PK.contains(columns::OL_NUMBER));
    assert!(OrderLine::SQL_INSERT.contains("INSERT INTO order_line"));

    // Datum / DatumDyn metadata
    let data_type = OrderLine::data_type();
    assert_eq!(object.type_family().unwrap(), data_type.type_family());

    // Tuple / value / binary / textual roundtrips
    let tuple = object.to_tuple().unwrap();
    assert_same_order_line(&OrderLine::from_tuple(&tuple).unwrap(), &object);
    let value = object.to_value(&data_type).unwrap();
    assert_same_order_line(&OrderLine::from_value(&value).unwrap(), &object);
    let binary = object.to_binary(&data_type).unwrap();
    assert_same_order_line(&OrderLine::from_binary(binary.as_ref()).unwrap(), &object);
    let textual = object.to_textual(&data_type).unwrap();
    assert_same_order_line(&OrderLine::from_textual(textual.as_str()).unwrap(), &object);

    // Clone through DatumDyn
    let cloned: Box<dyn DatumDyn> = object.clone_boxed();
    let cloned_value = cloned.to_value(&data_type).unwrap();
    assert_same_order_line(&OrderLine::from_value(&cloned_value).unwrap(), &object);

    // Insert parameters follow the column declaration order
    let params = object.insert_params();
    assert_eq!(params.0, 1);
    assert_eq!(params.6.as_deref(), Some("delivery_val"));
    assert_eq!(params.7, 10);

    // Partial update statement over the four-column primary key
    let update = OrderLineChange {
        ol_delivery_d: FieldChange::Set(Some("new_delivery".to_string())),
        ol_quantity: FieldChange::Set(20),
        ..OrderLineChange::default()
    };
    let (sql, update_params) = update.update_by_pk(1, 2, 3, 4).unwrap();
    assert_eq!(
        sql,
        "UPDATE order_line SET ol_delivery_d = ?, ol_quantity = ? WHERE ol_o_id = ? AND ol_d_id = ? AND ol_w_id = ? AND ol_number = ?"
    );
    assert_eq!(update_params.len(), 6);
    assert!(OrderLineChange::default().is_empty());
    assert!(
        OrderLineChange::default()
            .update_by_pk(1, 2, 3, 4)
            .is_none()
    );

    // Set(None) on a nullable column emits `<col> = ?` with a NULL parameter
    let null_update = OrderLineChange {
        ol_delivery_d: FieldChange::Set(None),
        ..OrderLineChange::default()
    };
    let (null_sql, null_params) = null_update.update_by_pk(1, 2, 3, 4).unwrap();
    assert_eq!(
        null_sql,
        "UPDATE order_line SET ol_delivery_d = ? WHERE ol_o_id = ? AND ol_d_id = ? AND ol_w_id = ? AND ol_number = ?"
    );
    assert_eq!(null_params.len(), 5);
    assert!(
        null_params[0]
            .to_value(&String::data_type())
            .unwrap()
            .is_null()
    );
}

#[test]
fn order_line_nullable_fields_roundtrip() {
    let object = OrderLine::new(1, 2, 3, 4, 5, 6, None, 10, Numeric::from(100i32));
    let tuple = object.to_tuple().unwrap();
    assert!(tuple.is_null(6));
    let restored = OrderLine::from_tuple(&tuple).unwrap();
    assert_same_order_line(&restored, &object);

    let value = object.to_tuple_value().unwrap();
    assert!(value.values()[6].is_null());
    let restored = OrderLine::from_tuple_value(&value).unwrap();
    assert_same_order_line(&restored, &object);
}
