//! Unit tests for the `generated::stock::Stock` entity.

use mududb::contract::database::entity::Entity;
use mududb::contract::database::field_change::FieldChange;
use mududb::types::datum::{Datum, DatumDyn};

use crate::generated::stock::object::{Stock, StockChange, columns};

fn sample_stock() -> Stock {
    Stock::new(1, 2, 100, 1000, 5, 1)
}

fn assert_same_stock(lhs: &Stock, rhs: &Stock) {
    assert_eq!(lhs.s_i_id, rhs.s_i_id);
    assert_eq!(lhs.s_w_id, rhs.s_w_id);
    assert_eq!(lhs.s_quantity, rhs.s_quantity);
    assert_eq!(lhs.s_ytd, rhs.s_ytd);
    assert_eq!(lhs.s_order_cnt, rhs.s_order_cnt);
    assert_eq!(lhs.s_remote_cnt, rhs.s_remote_cnt);
}

#[test]
fn stock_lifecycle() {
    let entity = sample_stock();
    assert_eq!(entity.s_i_id, 1);
    assert_eq!(entity.s_quantity, 100);

    // Entity metadata
    assert_eq!(Stock::table_name(), "stock");
    assert_eq!(Stock::TABLE_NAME, "stock");
    assert_eq!(Stock::tuple_desc().fields().len(), 6);
    assert_eq!(Stock::tuple_desc().fields()[0].name(), columns::S_I_ID);
    assert!(Stock::SQL_GET_BY_PK.contains(columns::S_W_ID));
    assert!(Stock::SQL_INSERT.contains("INSERT INTO stock"));

    // Datum / DatumDyn metadata
    let data_type = Stock::data_type();
    assert_eq!(entity.type_family().unwrap(), data_type.type_family());

    // Tuple / value / binary / textual roundtrips
    let tuple = entity.to_tuple().unwrap();
    assert_same_stock(&Stock::from_tuple(&tuple).unwrap(), &entity);
    let value = entity.to_value(&data_type).unwrap();
    assert_same_stock(&Stock::from_value(&value).unwrap(), &entity);
    let binary = entity.to_binary(&data_type).unwrap();
    assert_same_stock(&Stock::from_binary(binary.as_ref()).unwrap(), &entity);
    let textual = entity.to_textual(&data_type).unwrap();
    assert_same_stock(&Stock::from_textual(textual.as_str()).unwrap(), &entity);

    // Clone through DatumDyn
    let cloned: Box<dyn DatumDyn> = entity.clone_boxed();
    let cloned_value = cloned.to_value(&data_type).unwrap();
    assert_same_stock(&Stock::from_value(&cloned_value).unwrap(), &entity);

    // Insert parameters follow the column declaration order
    let params = entity.insert_params();
    assert_eq!(params.0, 1);
    assert_eq!(params.2, 100);
    assert_eq!(params.5, 1);

    // Partial update statement over the composite primary key
    let update = StockChange {
        s_quantity: FieldChange::Set(90),
        s_order_cnt: FieldChange::Set(6),
        ..StockChange::default()
    };
    let (sql, update_params) = update.update_by_pk(1, 2).unwrap();
    assert_eq!(
        sql,
        "UPDATE stock SET s_quantity = ?, s_order_cnt = ? WHERE s_i_id = ? AND s_w_id = ?"
    );
    assert_eq!(update_params.len(), 4);
    assert!(StockChange::default().is_empty());
    assert!(StockChange::default().update_by_pk(1, 2).is_none());
}
