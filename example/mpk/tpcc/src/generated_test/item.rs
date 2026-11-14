//! Unit tests for the `generated::item::Item` entity.

use mududb::contract::database::entity::Entity;
use mududb::contract::database::field_change::FieldChange;
use mududb::mudu::data_type::numeric::Numeric;
use mududb::types::datum::{Datum, DatumDyn};

use crate::generated::item::object::{Item, ItemChange, columns};

fn sample_item() -> Item {
    Item::new(1, "i_name_val".to_string(), Numeric::from(99i32))
}

fn assert_same_item(lhs: &Item, rhs: &Item) {
    assert_eq!(lhs.i_id, rhs.i_id);
    assert_eq!(lhs.i_name, rhs.i_name);
    assert_eq!(lhs.i_price, rhs.i_price);
}

#[test]
fn item_lifecycle() {
    let entity = sample_item();
    assert_eq!(entity.i_id, 1);
    assert_eq!(entity.i_name, "i_name_val");

    // Entity metadata
    assert_eq!(Item::table_name(), "item");
    assert_eq!(Item::TABLE_NAME, "item");
    assert_eq!(Item::tuple_desc().fields().len(), 3);
    assert_eq!(Item::tuple_desc().fields()[0].name(), columns::I_ID);
    assert!(Item::SQL_GET_BY_PK.contains(columns::I_ID));
    assert!(Item::SQL_INSERT.contains("INSERT INTO item"));

    // Datum / DatumDyn metadata
    let data_type = Item::data_type();
    assert_eq!(entity.type_family().unwrap(), data_type.type_family());

    // Tuple / value / binary / textual roundtrips
    let tuple = entity.to_tuple().unwrap();
    assert_same_item(&Item::from_tuple(&tuple).unwrap(), &entity);
    let value = entity.to_value(&data_type).unwrap();
    assert_same_item(&Item::from_value(&value).unwrap(), &entity);
    let binary = entity.to_binary(&data_type).unwrap();
    assert_same_item(&Item::from_binary(binary.as_ref()).unwrap(), &entity);
    let textual = entity.to_textual(&data_type).unwrap();
    assert_same_item(&Item::from_textual(textual.as_str()).unwrap(), &entity);

    // Clone through DatumDyn
    let cloned: Box<dyn DatumDyn> = entity.clone_boxed();
    let cloned_value = cloned.to_value(&data_type).unwrap();
    assert_same_item(&Item::from_value(&cloned_value).unwrap(), &entity);

    // Insert parameters follow the column declaration order
    let params = entity.insert_params();
    assert_eq!(params.0, 1);
    assert_eq!(params.1, "i_name_val");

    // Partial update statement
    let update = ItemChange {
        i_name: FieldChange::Set("new_name".to_string()),
        i_price: FieldChange::Set(Numeric::from(199i32)),
    };
    let (sql, update_params) = update.update_by_pk(1).unwrap();
    assert_eq!(
        sql,
        "UPDATE item SET i_name = ?, i_price = ? WHERE i_id = ?"
    );
    assert_eq!(update_params.len(), 3);
    assert!(ItemChange::default().is_empty());
    assert!(ItemChange::default().update_by_pk(1).is_none());
}
