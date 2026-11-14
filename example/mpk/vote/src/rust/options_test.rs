//! Unit tests for the `rust::options::Options` entity.

use mududb::contract::database::entity::Entity;
use mududb::contract::database::field_change::FieldChange;
use mududb::types::datum::{Datum, DatumDyn};

use crate::rust::options::object::{Options, OptionsChange, columns};

fn sample_options() -> Options {
    Options::new(
        "option_id_val".to_string(),
        Some("vote_id_val".to_string()),
        "option_text_val".to_string(),
    )
}

fn assert_same_options(lhs: &Options, rhs: &Options) {
    assert_eq!(lhs.option_id, rhs.option_id);
    assert_eq!(lhs.vote_id, rhs.vote_id);
    assert_eq!(lhs.option_text, rhs.option_text);
}

#[test]
fn options_lifecycle() {
    let entity = sample_options();
    assert_eq!(entity.option_id, "option_id_val");
    assert_eq!(entity.option_text, "option_text_val");

    // Entity metadata
    assert_eq!(Options::table_name(), "options");
    assert_eq!(Options::TABLE_NAME, "options");
    assert_eq!(Options::tuple_desc().fields().len(), 3);
    assert_eq!(Options::tuple_desc().fields()[0].name(), columns::OPTION_ID);
    assert!(Options::SQL_GET_BY_PK.contains(columns::OPTION_ID));
    assert!(Options::SQL_INSERT.contains("INSERT INTO options"));

    // Datum / DatumDyn metadata
    let data_type = Options::data_type();
    assert_eq!(entity.type_family().unwrap(), data_type.type_family());

    // Tuple roundtrip
    let tuple = entity.to_tuple().unwrap();
    let from_tuple = Options::from_tuple(&tuple).unwrap();
    assert_same_options(&from_tuple, &entity);

    // Value roundtrip
    let value = entity.to_value(&data_type).unwrap();
    let from_value = Options::from_value(&value).unwrap();
    assert_same_options(&from_value, &entity);

    // Binary roundtrip
    let binary = entity.to_binary(&data_type).unwrap();
    let from_binary = Options::from_binary(binary.as_ref()).unwrap();
    assert_same_options(&from_binary, &entity);

    // Textual roundtrip
    let textual = entity.to_textual(&data_type).unwrap();
    let from_textual = Options::from_textual(textual.as_str()).unwrap();
    assert_same_options(&from_textual, &entity);

    // Clone through DatumDyn
    let cloned: Box<dyn DatumDyn> = entity.clone_boxed();
    let cloned_value = cloned.to_value(&data_type).unwrap();
    let from_cloned = Options::from_value(&cloned_value).unwrap();
    assert_same_options(&from_cloned, &entity);

    // Insert parameters follow the column declaration order
    let params = entity.insert_params();
    assert_eq!(params.0, "option_id_val");
    assert_eq!(params.1.as_deref(), Some("vote_id_val"));
    assert_eq!(params.2, "option_text_val");

    // Partial update statement
    let update = OptionsChange {
        vote_id: FieldChange::Set(Some("new_vote".to_string())),
        option_text: FieldChange::Set("new_text".to_string()),
    };
    let (sql, update_params) = update.update_by_pk("option_id_val".to_string()).unwrap();
    assert_eq!(
        sql,
        "UPDATE options SET vote_id = ?, option_text = ? WHERE option_id = ?"
    );
    assert_eq!(update_params.len(), 3);
    assert!(OptionsChange::default().is_empty());
    assert!(
        OptionsChange::default()
            .update_by_pk("option_id_val".to_string())
            .is_none()
    );

    // Set(None) on a nullable column emits `<col> = ?` with a NULL parameter
    let null_update = OptionsChange {
        vote_id: FieldChange::Set(None),
        ..OptionsChange::default()
    };
    let (null_sql, null_params) = null_update
        .update_by_pk("option_id_val".to_string())
        .unwrap();
    assert_eq!(
        null_sql,
        "UPDATE options SET vote_id = ? WHERE option_id = ?"
    );
    assert_eq!(null_params.len(), 2);
    assert!(
        null_params[0]
            .to_value(&String::data_type())
            .unwrap()
            .is_null()
    );
}

#[test]
fn options_nullable_fields_roundtrip() {
    let entity = Options::new(
        "option_id_val".to_string(),
        None,
        "option_text_val".to_string(),
    );
    let tuple = entity.to_tuple().unwrap();
    assert!(tuple.is_null(1));
    let restored = Options::from_tuple(&tuple).unwrap();
    assert_same_options(&restored, &entity);

    let value = entity.to_tuple_value().unwrap();
    assert!(value.values()[1].is_null());
    let restored = Options::from_tuple_value(&value).unwrap();
    assert_same_options(&restored, &entity);
}
