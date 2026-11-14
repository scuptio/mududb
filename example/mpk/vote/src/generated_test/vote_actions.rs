//! Unit tests for the `generated::vote_actions::VoteActions` entity.

use mududb::contract::database::entity::Entity;
use mududb::contract::database::field_change::FieldChange;
use mududb::types::datum::{Datum, DatumDyn};

use crate::generated::vote_actions::object::{VoteActions, VoteActionsChange, columns};

fn sample_vote_actions() -> VoteActions {
    VoteActions::new(
        "action_id_val".to_string(),
        Some("user_id_val".to_string()),
        Some("vote_id_val".to_string()),
        1,
        Some(0),
    )
}

fn assert_same_vote_actions(lhs: &VoteActions, rhs: &VoteActions) {
    assert_eq!(lhs.action_id, rhs.action_id);
    assert_eq!(lhs.user_id, rhs.user_id);
    assert_eq!(lhs.vote_id, rhs.vote_id);
    assert_eq!(lhs.action_time, rhs.action_time);
    assert_eq!(lhs.is_withdrawn, rhs.is_withdrawn);
}

#[test]
fn vote_actions_lifecycle() {
    let entity = sample_vote_actions();
    assert_eq!(entity.action_id, "action_id_val");
    assert_eq!(entity.action_time, 1);

    // Entity metadata
    assert_eq!(VoteActions::table_name(), "vote_actions");
    assert_eq!(VoteActions::TABLE_NAME, "vote_actions");
    assert_eq!(VoteActions::tuple_desc().fields().len(), 5);
    assert_eq!(
        VoteActions::tuple_desc().fields()[0].name(),
        columns::ACTION_ID
    );
    assert!(VoteActions::SQL_GET_BY_PK.contains(columns::ACTION_ID));
    assert!(VoteActions::SQL_INSERT.contains("INSERT INTO vote_actions"));

    // Datum / DatumDyn metadata
    let data_type = VoteActions::data_type();
    assert_eq!(entity.type_family().unwrap(), data_type.type_family());

    // Tuple roundtrip
    let tuple = entity.to_tuple().unwrap();
    let from_tuple = VoteActions::from_tuple(&tuple).unwrap();
    assert_same_vote_actions(&from_tuple, &entity);

    // Value roundtrip
    let value = entity.to_value(&data_type).unwrap();
    let from_value = VoteActions::from_value(&value).unwrap();
    assert_same_vote_actions(&from_value, &entity);

    // Binary roundtrip
    let binary = entity.to_binary(&data_type).unwrap();
    let from_binary = VoteActions::from_binary(binary.as_ref()).unwrap();
    assert_same_vote_actions(&from_binary, &entity);

    // Textual roundtrip
    let textual = entity.to_textual(&data_type).unwrap();
    let from_textual = VoteActions::from_textual(textual.as_str()).unwrap();
    assert_same_vote_actions(&from_textual, &entity);

    // Clone through DatumDyn
    let cloned: Box<dyn DatumDyn> = entity.clone_boxed();
    let cloned_value = cloned.to_value(&data_type).unwrap();
    let from_cloned = VoteActions::from_value(&cloned_value).unwrap();
    assert_same_vote_actions(&from_cloned, &entity);

    // Insert parameters follow the column declaration order
    let params = entity.insert_params();
    assert_eq!(params.0, "action_id_val");
    assert_eq!(params.1.as_deref(), Some("user_id_val"));
    assert_eq!(params.3, 1);

    // Partial update statement
    let update = VoteActionsChange {
        vote_id: FieldChange::Set(Some("new_vote".to_string())),
        is_withdrawn: FieldChange::Set(Some(1)),
        ..VoteActionsChange::default()
    };
    let (sql, update_params) = update.update_by_pk("action_id_val".to_string()).unwrap();
    assert_eq!(
        sql,
        "UPDATE vote_actions SET vote_id = ?, is_withdrawn = ? WHERE action_id = ?"
    );
    assert_eq!(update_params.len(), 3);
    assert!(VoteActionsChange::default().is_empty());
    assert!(
        VoteActionsChange::default()
            .update_by_pk("action_id_val".to_string())
            .is_none()
    );

    // Set(None) on a nullable column emits `<col> = ?` with a NULL parameter
    let null_update = VoteActionsChange {
        user_id: FieldChange::Set(None),
        ..VoteActionsChange::default()
    };
    let (null_sql, null_params) = null_update
        .update_by_pk("action_id_val".to_string())
        .unwrap();
    assert_eq!(
        null_sql,
        "UPDATE vote_actions SET user_id = ? WHERE action_id = ?"
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
fn vote_actions_nullable_fields_roundtrip() {
    let entity = VoteActions::new("action_id_val".to_string(), None, None, 1, None);
    let tuple = entity.to_tuple().unwrap();
    assert!(tuple.is_null(1));
    assert!(tuple.is_null(4));
    let restored = VoteActions::from_tuple(&tuple).unwrap();
    assert_same_vote_actions(&restored, &entity);

    let value = entity.to_tuple_value().unwrap();
    assert!(value.values()[1].is_null());
    let restored = VoteActions::from_tuple_value(&value).unwrap();
    assert_same_vote_actions(&restored, &entity);
}
