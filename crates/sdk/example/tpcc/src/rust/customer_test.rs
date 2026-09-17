//! Unit tests for the `rust::customer::Customer` entity.

use mududb::contract::database::entity::Entity;
use mududb::mudu::data_type::numeric::Numeric;
use mududb::types::datum::{Datum, DatumDyn};

use crate::rust::customer::object::Customer;

fn sample_customer() -> Customer {
    Customer::new(
        1,
        1,
        1,
        "c_first_val".to_string(),
        "c_last_val".to_string(),
        1,
        "c_credit_val".to_string(),
        Numeric::from(1i32),
        Numeric::from(1i32),
        1,
        1,
        Some(1),
    )
}

fn assert_same_customer(lhs: &Customer, rhs: &Customer) {
    assert_eq!(lhs.c_id, rhs.c_id);
    assert_eq!(lhs.c_d_id, rhs.c_d_id);
    assert_eq!(lhs.c_w_id, rhs.c_w_id);
    assert_eq!(lhs.c_first, rhs.c_first);
    assert_eq!(lhs.c_last, rhs.c_last);
    assert_eq!(lhs.c_discount, rhs.c_discount);
    assert_eq!(lhs.c_credit, rhs.c_credit);
    assert_eq!(lhs.c_balance, rhs.c_balance);
    assert_eq!(lhs.c_ytd_payment, rhs.c_ytd_payment);
    assert_eq!(lhs.c_payment_cnt, rhs.c_payment_cnt);
    assert_eq!(lhs.c_delivery_cnt, rhs.c_delivery_cnt);
    assert_eq!(lhs.c_last_order_id, rhs.c_last_order_id);
}

#[test]
fn customer_lifecycle() {
    let entity = sample_customer();
    assert_eq!(entity.c_id, 1);
    assert_eq!(entity.c_first, "c_first_val");
    assert_eq!(entity.c_last_order_id, Some(1));

    // Entity metadata
    assert_eq!(Customer::table_name(), "customer");
    assert_eq!(Customer::TABLE_NAME, "customer");
    assert_eq!(Customer::tuple_desc().fields().len(), 12);
    assert!(Customer::SQL_GET_BY_PK.contains("c_w_id = ?"));

    // Datum / DatumDyn metadata
    let data_type = Customer::data_type();
    assert_eq!(entity.type_family().unwrap(), data_type.type_family());

    // Tuple roundtrip
    let tuple = entity.to_tuple().unwrap();
    let from_tuple = Customer::from_tuple(&tuple).unwrap();
    assert_same_customer(&from_tuple, &entity);

    // Value roundtrip
    let value = entity.to_value(&data_type).unwrap();
    let from_value = Customer::from_value(&value).unwrap();
    assert_same_customer(&from_value, &entity);

    // Binary roundtrip
    let binary = entity.to_binary(&data_type).unwrap();
    let from_binary = Customer::from_binary(binary.as_ref()).unwrap();
    assert_same_customer(&from_binary, &entity);

    // Textual roundtrip
    let textual = entity.to_textual(&data_type).unwrap();
    let from_textual = Customer::from_textual(textual.as_str()).unwrap();
    assert_same_customer(&from_textual, &entity);

    // Clone through DatumDyn
    let cloned: Box<dyn DatumDyn> = entity.clone_boxed();
    let cloned_value = cloned.to_value(&data_type).unwrap();
    let from_cloned = Customer::from_value(&cloned_value).unwrap();
    assert_same_customer(&from_cloned, &entity);
}

#[test]
fn customer_nullable_last_order_id_roundtrip() {
    let mut entity = sample_customer();
    entity.c_last_order_id = None;

    let tuple = entity.to_tuple().unwrap();
    assert!(tuple.is_null(11));
    let from_tuple = Customer::from_tuple(&tuple).unwrap();
    assert_eq!(from_tuple.c_last_order_id, None);

    let value = entity.to_tuple_value().unwrap();
    assert!(value.values()[11].is_null());
    let from_value = Customer::from_tuple_value(&value).unwrap();
    assert_eq!(from_value.c_last_order_id, None);
}
