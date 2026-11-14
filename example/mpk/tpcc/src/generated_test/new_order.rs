//! Unit tests for the `generated::new_order::NewOrder` entity.

use mududb::contract::database::entity::Entity;
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
    let entity = sample_new_order();
    assert_eq!(entity.no_o_id, 1);
    assert_eq!(entity.no_w_id, 3);

    // Entity metadata (insert-only queue table: no changeset API)
    assert_eq!(NewOrder::table_name(), "new_order");
    assert_eq!(NewOrder::TABLE_NAME, "new_order");
    assert_eq!(NewOrder::tuple_desc().fields().len(), 3);
    assert_eq!(NewOrder::tuple_desc().fields()[0].name(), columns::NO_O_ID);
    assert!(NewOrder::SQL_INSERT.contains("INSERT INTO new_order"));

    // Datum / DatumDyn metadata
    let data_type = NewOrder::data_type();
    assert_eq!(entity.type_family().unwrap(), data_type.type_family());

    // Tuple / value / binary / textual roundtrips
    let tuple = entity.to_tuple().unwrap();
    assert_same_new_order(&NewOrder::from_tuple(&tuple).unwrap(), &entity);
    let value = entity.to_value(&data_type).unwrap();
    assert_same_new_order(&NewOrder::from_value(&value).unwrap(), &entity);
    let binary = entity.to_binary(&data_type).unwrap();
    assert_same_new_order(&NewOrder::from_binary(binary.as_ref()).unwrap(), &entity);
    let textual = entity.to_textual(&data_type).unwrap();
    assert_same_new_order(&NewOrder::from_textual(textual.as_str()).unwrap(), &entity);

    // Clone through DatumDyn
    let cloned: Box<dyn DatumDyn> = entity.clone_boxed();
    let cloned_value = cloned.to_value(&data_type).unwrap();
    assert_same_new_order(&NewOrder::from_value(&cloned_value).unwrap(), &entity);

    // Insert parameters follow the column declaration order
    let params = entity.insert_params();
    assert_eq!(params, (1, 2, 3));
}
