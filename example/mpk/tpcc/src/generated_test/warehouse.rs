//! Unit tests for the `generated::warehouse::Warehouse` entity.

use mududb::contract::database::entity::Entity;
use mududb::contract::database::field_change::FieldChange;
use mududb::mudu::data_type::numeric::Numeric;
use mududb::types::datum::{Datum, DatumDyn};

use crate::generated::warehouse::object::{Warehouse, WarehouseChange, columns};

fn sample_warehouse() -> Warehouse {
    Warehouse::new(1, "w_name_val".to_string(), 10, Numeric::from(300000i32))
}

fn assert_same_warehouse(lhs: &Warehouse, rhs: &Warehouse) {
    assert_eq!(lhs.w_id, rhs.w_id);
    assert_eq!(lhs.w_name, rhs.w_name);
    assert_eq!(lhs.w_tax, rhs.w_tax);
    assert_eq!(lhs.w_ytd, rhs.w_ytd);
}

#[test]
fn warehouse_lifecycle() {
    let entity = sample_warehouse();
    assert_eq!(entity.w_id, 1);
    assert_eq!(entity.w_tax, 10);

    // Entity metadata
    assert_eq!(Warehouse::table_name(), "warehouse");
    assert_eq!(Warehouse::TABLE_NAME, "warehouse");
    assert_eq!(Warehouse::tuple_desc().fields().len(), 4);
    assert_eq!(Warehouse::tuple_desc().fields()[0].name(), columns::W_ID);
    assert!(Warehouse::SQL_GET_BY_PK.contains(columns::W_ID));
    assert!(Warehouse::SQL_INSERT.contains("INSERT INTO warehouse"));

    // Datum / DatumDyn metadata
    let data_type = Warehouse::data_type();
    assert_eq!(entity.type_family().unwrap(), data_type.type_family());

    // Tuple / value / binary / textual roundtrips
    let tuple = entity.to_tuple().unwrap();
    assert_same_warehouse(&Warehouse::from_tuple(&tuple).unwrap(), &entity);
    let value = entity.to_value(&data_type).unwrap();
    assert_same_warehouse(&Warehouse::from_value(&value).unwrap(), &entity);
    let binary = entity.to_binary(&data_type).unwrap();
    assert_same_warehouse(&Warehouse::from_binary(binary.as_ref()).unwrap(), &entity);
    let textual = entity.to_textual(&data_type).unwrap();
    assert_same_warehouse(&Warehouse::from_textual(textual.as_str()).unwrap(), &entity);

    // Clone through DatumDyn
    let cloned: Box<dyn DatumDyn> = entity.clone_boxed();
    let cloned_value = cloned.to_value(&data_type).unwrap();
    assert_same_warehouse(&Warehouse::from_value(&cloned_value).unwrap(), &entity);

    // Insert parameters follow the column declaration order
    let params = entity.insert_params();
    assert_eq!(params.0, 1);
    assert_eq!(params.1, "w_name_val");
    assert_eq!(params.2, 10);

    // Partial update statement
    let update = WarehouseChange {
        w_name: FieldChange::Set("new_name".to_string()),
        w_ytd: FieldChange::Set(Numeric::from(300100i32)),
        ..WarehouseChange::default()
    };
    let (sql, update_params) = update.update_by_pk(1).unwrap();
    assert_eq!(
        sql,
        "UPDATE warehouse SET w_name = ?, w_ytd = ? WHERE w_id = ?"
    );
    assert_eq!(update_params.len(), 3);
    assert!(WarehouseChange::default().is_empty());
    assert!(WarehouseChange::default().update_by_pk(1).is_none());
}
