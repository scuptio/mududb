//! Unit tests for the `generated::history::History` object.

use mududb::contract::database::field_change::FieldChange;
use mududb::contract::database::object::Object;
use mududb::mudu::data_type::numeric::Numeric;
use mududb::types::datum::{Datum, DatumDyn};

use crate::generated::history::object::{History, HistoryChange, columns};

fn sample_history() -> History {
    History::new(
        "h_id_val".to_string(),
        1,
        2,
        3,
        4,
        5,
        Numeric::from(100i32),
        "h_data_val".to_string(),
    )
}

fn assert_same_history(lhs: &History, rhs: &History) {
    assert_eq!(lhs.h_id, rhs.h_id);
    assert_eq!(lhs.h_c_id, rhs.h_c_id);
    assert_eq!(lhs.h_c_d_id, rhs.h_c_d_id);
    assert_eq!(lhs.h_c_w_id, rhs.h_c_w_id);
    assert_eq!(lhs.h_d_id, rhs.h_d_id);
    assert_eq!(lhs.h_w_id, rhs.h_w_id);
    assert_eq!(lhs.h_amount, rhs.h_amount);
    assert_eq!(lhs.h_data, rhs.h_data);
}

#[test]
fn history_lifecycle() {
    let object = sample_history();
    assert_eq!(object.h_id, "h_id_val");
    assert_eq!(object.h_w_id, 5);

    // Object metadata
    assert_eq!(History::class_name(), "history");
    assert_eq!(History::TABLE_NAME, "history");
    assert_eq!(History::tuple_desc().fields().len(), 8);
    assert_eq!(History::tuple_desc().fields()[0].name(), columns::H_ID);
    assert!(History::SQL_GET_BY_PK.contains(columns::H_ID));
    assert!(History::SQL_INSERT.contains("INSERT INTO history"));

    // Datum / DatumDyn metadata
    let data_type = History::data_type();
    assert_eq!(object.type_family().unwrap(), data_type.type_family());

    // Tuple / value / binary / textual roundtrips
    let tuple = object.to_tuple().unwrap();
    assert_same_history(&History::from_tuple(&tuple).unwrap(), &object);
    let value = object.to_value(&data_type).unwrap();
    assert_same_history(&History::from_value(&value).unwrap(), &object);
    let binary = object.to_binary(&data_type).unwrap();
    assert_same_history(&History::from_binary(binary.as_ref()).unwrap(), &object);
    let textual = object.to_textual(&data_type).unwrap();
    assert_same_history(&History::from_textual(textual.as_str()).unwrap(), &object);

    // Clone through DatumDyn
    let cloned: Box<dyn DatumDyn> = object.clone_boxed();
    let cloned_value = cloned.to_value(&data_type).unwrap();
    assert_same_history(&History::from_value(&cloned_value).unwrap(), &object);

    // Insert parameters follow the column declaration order
    let params = object.insert_params();
    assert_eq!(params.0, "h_id_val");
    assert_eq!(params.1, 1);
    assert_eq!(params.7, "h_data_val");

    // Partial update statement
    let update = HistoryChange {
        h_amount: FieldChange::Set(Numeric::from(200i32)),
        h_data: FieldChange::Set("new_data".to_string()),
        ..HistoryChange::default()
    };
    let (sql, update_params) = update.update_by_pk("h_id_val".to_string()).unwrap();
    assert_eq!(
        sql,
        "UPDATE history SET h_amount = ?, h_data = ? WHERE h_id = ?"
    );
    assert_eq!(update_params.len(), 3);
    assert!(HistoryChange::default().is_empty());
    assert!(
        HistoryChange::default()
            .update_by_pk("h_id_val".to_string())
            .is_none()
    );
}
