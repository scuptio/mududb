//! Unit tests for the `generated::new_order::NewOrder` object.

use mududb::contract::database::object::Object;
use mududb::types::datum::{Datum, DatumDyn};

use crate::generated::new_order::object::{NewOrder, columns};

fn sample_new_order() -> NewOrder {
    NewOrder::new(1, 2, 3)
}

fn assert_same_new_order(lhs: &NewOrder, rhs: &NewOrder) {
    assert_eq!(lhs.no_o_id, rhs.no_o_id);
    assert_eq!(lhs.no_d_id, rhs.no_d_id);
    assert_eq!(lhs.no_w_id, rhs.no_w_id);
}

#[test]
fn new_order_lifecycle() {
    let object = sample_new_order();
    assert_eq!(object.no_o_id, 1);
    assert_eq!(object.no_w_id, 3);

    // Object metadata (insert-only queue table: no changeset API)
    assert_eq!(NewOrder::class_name(), "new_order");
    assert_eq!(NewOrder::TABLE_NAME, "new_order");
    assert_eq!(NewOrder::tuple_desc().fields().len(), 3);
    assert_eq!(NewOrder::tuple_desc().fields()[0].name(), columns::NO_O_ID);
    assert!(NewOrder::SQL_INSERT.contains("INSERT INTO new_order"));

    // Datum / DatumDyn metadata
    let data_type = NewOrder::data_type();
    assert_eq!(object.type_family().unwrap(), data_type.type_family());

    // Tuple / value / binary / textual roundtrips
    let tuple = object.to_tuple().unwrap();
    assert_same_new_order(&NewOrder::from_tuple(&tuple).unwrap(), &object);
    let value = object.to_value(&data_type).unwrap();
    assert_same_new_order(&NewOrder::from_value(&value).unwrap(), &object);
    let binary = object.to_binary(&data_type).unwrap();
    assert_same_new_order(&NewOrder::from_binary(binary.as_ref()).unwrap(), &object);
    let textual = object.to_textual(&data_type).unwrap();
    assert_same_new_order(&NewOrder::from_textual(textual.as_str()).unwrap(), &object);

    // Clone through DatumDyn
    let cloned: Box<dyn DatumDyn> = object.clone_boxed();
    let cloned_value = cloned.to_value(&data_type).unwrap();
    assert_same_new_order(&NewOrder::from_value(&cloned_value).unwrap(), &object);

    // Insert parameters follow the column declaration order
    let params = object.insert_params();
    assert_eq!(params, (1, 2, 3));
}
