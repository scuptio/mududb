//! Unit tests for the `rust::vote_choices::VoteChoices` object.

use mududb::contract::database::field_change::FieldChange;
use mududb::contract::database::object::Object;
use mududb::types::datum::{Datum, DatumDyn};

use crate::rust::vote_choices::object::{VoteChoices, VoteChoicesChange, columns};

fn sample_vote_choices() -> VoteChoices {
    VoteChoices::new(
        "choice_id_val".to_string(),
        Some("action_id_val".to_string()),
        Some("option_id_val".to_string()),
    )
}

fn assert_same_vote_choices(lhs: &VoteChoices, rhs: &VoteChoices) {
    assert_eq!(lhs.choice_id, rhs.choice_id);
    assert_eq!(lhs.action_id, rhs.action_id);
    assert_eq!(lhs.option_id, rhs.option_id);
}

#[test]
fn vote_choices_lifecycle() {
    let object = sample_vote_choices();
    assert_eq!(object.choice_id, "choice_id_val");
    assert_eq!(object.option_id.as_deref(), Some("option_id_val"));

    // Object metadata
    assert_eq!(VoteChoices::class_name(), "vote_choices");
    assert_eq!(VoteChoices::TABLE_NAME, "vote_choices");
    assert_eq!(VoteChoices::tuple_desc().fields().len(), 3);
    assert_eq!(
        VoteChoices::tuple_desc().fields()[0].name(),
        columns::CHOICE_ID
    );
    assert!(VoteChoices::SQL_GET_BY_PK.contains(columns::CHOICE_ID));
    assert!(VoteChoices::SQL_INSERT.contains("INSERT INTO vote_choices"));

    // Datum / DatumDyn metadata
    let data_type = VoteChoices::data_type();
    assert_eq!(object.type_family().unwrap(), data_type.type_family());

    // Tuple roundtrip
    let tuple = object.to_tuple().unwrap();
    let from_tuple = VoteChoices::from_tuple(&tuple).unwrap();
    assert_same_vote_choices(&from_tuple, &object);

    // Value roundtrip
    let value = object.to_value(&data_type).unwrap();
    let from_value = VoteChoices::from_value(&value).unwrap();
    assert_same_vote_choices(&from_value, &object);

    // Binary roundtrip
    let binary = object.to_binary(&data_type).unwrap();
    let from_binary = VoteChoices::from_binary(binary.as_ref()).unwrap();
    assert_same_vote_choices(&from_binary, &object);

    // Textual roundtrip
    let textual = object.to_textual(&data_type).unwrap();
    let from_textual = VoteChoices::from_textual(textual.as_str()).unwrap();
    assert_same_vote_choices(&from_textual, &object);

    // Clone through DatumDyn
    let cloned: Box<dyn DatumDyn> = object.clone_boxed();
    let cloned_value = cloned.to_value(&data_type).unwrap();
    let from_cloned = VoteChoices::from_value(&cloned_value).unwrap();
    assert_same_vote_choices(&from_cloned, &object);

    // Insert parameters follow the column declaration order
    let params = object.insert_params();
    assert_eq!(params.0, "choice_id_val");
    assert_eq!(params.1.as_deref(), Some("action_id_val"));
    assert_eq!(params.2.as_deref(), Some("option_id_val"));

    // Partial update statement
    let update = VoteChoicesChange {
        action_id: FieldChange::Set(Some("new_action".to_string())),
        option_id: FieldChange::Set(Some("new_option".to_string())),
    };
    let (sql, update_params) = update.update_by_pk("choice_id_val".to_string()).unwrap();
    assert_eq!(
        sql,
        "UPDATE vote_choices SET action_id = ?, option_id = ? WHERE choice_id = ?"
    );
    assert_eq!(update_params.len(), 3);
    assert!(VoteChoicesChange::default().is_empty());
    assert!(
        VoteChoicesChange::default()
            .update_by_pk("choice_id_val".to_string())
            .is_none()
    );

    // Set(None) on a nullable column emits `<col> = ?` with a NULL parameter
    let null_update = VoteChoicesChange {
        action_id: FieldChange::Set(None),
        ..VoteChoicesChange::default()
    };
    let (null_sql, null_params) = null_update
        .update_by_pk("choice_id_val".to_string())
        .unwrap();
    assert_eq!(
        null_sql,
        "UPDATE vote_choices SET action_id = ? WHERE choice_id = ?"
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
fn vote_choices_nullable_fields_roundtrip() {
    let object = VoteChoices::new("choice_id_val".to_string(), None, None);
    let tuple = object.to_tuple().unwrap();
    assert!(tuple.is_null(1));
    assert!(tuple.is_null(2));
    let restored = VoteChoices::from_tuple(&tuple).unwrap();
    assert_same_vote_choices(&restored, &object);

    let value = object.to_tuple_value().unwrap();
    assert!(value.values()[1].is_null());
    let restored = VoteChoices::from_tuple_value(&value).unwrap();
    assert_same_vote_choices(&restored, &object);
}
