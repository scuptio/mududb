//! Unit tests for the `rust::votes::Votes` object.

use mududb::contract::database::field_change::FieldChange;
use mududb::contract::database::object::Object;
use mududb::types::datum::{Datum, DatumDyn};

use crate::rust::votes::object::{Votes, VotesChange, columns};

fn sample_votes() -> Votes {
    Votes::new(
        "vote_id_val".to_string(),
        Some("creator_id_val".to_string()),
        "topic_val".to_string(),
        Some("vote_type_val".to_string()),
        Some(1),
        1,
        Some("visibility_rule_val".to_string()),
    )
}

fn assert_same_votes(lhs: &Votes, rhs: &Votes) {
    assert_eq!(lhs.vote_id, rhs.vote_id);
    assert_eq!(lhs.creator_id, rhs.creator_id);
    assert_eq!(lhs.topic, rhs.topic);
    assert_eq!(lhs.vote_type, rhs.vote_type);
    assert_eq!(lhs.max_choices, rhs.max_choices);
    assert_eq!(lhs.end_time, rhs.end_time);
    assert_eq!(lhs.visibility_rule, rhs.visibility_rule);
}

#[test]
fn votes_lifecycle() {
    let object = sample_votes();
    assert_eq!(object.vote_id, "vote_id_val");
    assert_eq!(object.topic, "topic_val");
    assert_eq!(object.end_time, 1);

    // Object metadata
    assert_eq!(Votes::class_name(), "votes");
    assert_eq!(Votes::TABLE_NAME, "votes");
    assert_eq!(Votes::tuple_desc().fields().len(), 7);

    // Datum / DatumDyn metadata
    let data_type = Votes::data_type();
    assert_eq!(object.type_family().unwrap(), data_type.type_family());

    // Tuple roundtrip
    let tuple = object.to_tuple().unwrap();
    let from_tuple = Votes::from_tuple(&tuple).unwrap();
    assert_same_votes(&from_tuple, &object);

    // Value roundtrip
    let value = object.to_value(&data_type).unwrap();
    let from_value = Votes::from_value(&value).unwrap();
    assert_same_votes(&from_value, &object);

    // Binary roundtrip
    let binary = object.to_binary(&data_type).unwrap();
    let from_binary = Votes::from_binary(binary.as_ref()).unwrap();
    assert_same_votes(&from_binary, &object);

    // Textual roundtrip
    let textual = object.to_textual(&data_type).unwrap();
    let from_textual = Votes::from_textual(textual.as_str()).unwrap();
    assert_same_votes(&from_textual, &object);

    // Clone through DatumDyn
    let cloned: Box<dyn DatumDyn> = object.clone_boxed();
    let cloned_value = cloned.to_value(&data_type).unwrap();
    let from_cloned = Votes::from_value(&cloned_value).unwrap();
    assert_same_votes(&from_cloned, &object);
}

#[test]
fn votes_nullable_fields_roundtrip() {
    let object = Votes::new(
        "vote_id_val".to_string(),
        None,
        "topic_val".to_string(),
        None,
        None,
        1,
        None,
    );
    let tuple = object.to_tuple().unwrap();
    assert!(tuple.is_null(1));
    let restored = Votes::from_tuple(&tuple).unwrap();
    assert_same_votes(&restored, &object);

    let value = object.to_tuple_value().unwrap();
    assert!(value.values()[1].is_null());
    let restored = Votes::from_tuple_value(&value).unwrap();
    assert_same_votes(&restored, &object);
}

#[test]
fn votes_insert_params_and_change() {
    let object = sample_votes();
    let params = object.insert_params();
    assert_eq!(params.0, "vote_id_val");
    assert_eq!(params.1.as_deref(), Some("creator_id_val"));
    assert_eq!(params.2, "topic_val");
    assert_eq!(params.5, 1);

    // Partial update statement
    let update = VotesChange {
        topic: FieldChange::Set("new_topic".to_string()),
        max_choices: FieldChange::Set(Some(3)),
        ..VotesChange::default()
    };
    let (sql, update_params) = update.update_by_pk("vote_id_val".to_string()).unwrap();
    assert_eq!(
        sql,
        "UPDATE votes SET topic = ?, max_choices = ? WHERE vote_id = ?"
    );
    assert_eq!(update_params.len(), 3);
    assert!(VotesChange::default().is_empty());
    assert!(
        VotesChange::default()
            .update_by_pk("vote_id_val".to_string())
            .is_none()
    );

    // Set(None) on a nullable column emits `<col> = ?` with a NULL parameter
    let null_update = VotesChange {
        creator_id: FieldChange::Set(None),
        ..VotesChange::default()
    };
    let (null_sql, null_params) = null_update.update_by_pk("vote_id_val".to_string()).unwrap();
    assert_eq!(
        null_sql,
        "UPDATE votes SET creator_id = ? WHERE vote_id = ?"
    );
    assert_eq!(null_params.len(), 2);
    assert!(
        null_params[0]
            .to_value(&String::data_type())
            .unwrap()
            .is_null()
    );

    // Column name constants feed the metadata and SQL builders
    assert_eq!(columns::VOTE_ID, "vote_id");
    assert!(Votes::SQL_DELETE_BY_PK.contains(columns::VOTE_ID));
}
