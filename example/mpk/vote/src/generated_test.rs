//! Unit tests for the `generated::votes::Votes` entity.

use mududb::contract::database::entity::Entity;
use mududb::types::datum::{Datum, DatumDyn};

use crate::generated::votes::object::Votes;

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
    let entity = sample_votes();
    assert_eq!(entity.vote_id, "vote_id_val");
    assert_eq!(entity.topic, "topic_val");
    assert_eq!(entity.end_time, 1);

    // Entity metadata
    assert_eq!(Votes::table_name(), "votes");
    assert_eq!(Votes::TABLE_NAME, "votes");
    assert_eq!(Votes::tuple_desc().fields().len(), 7);

    // Datum / DatumDyn metadata
    let data_type = Votes::data_type();
    assert_eq!(entity.type_family().unwrap(), data_type.type_family());

    // Tuple roundtrip
    let tuple = entity.to_tuple().unwrap();
    let from_tuple = Votes::from_tuple(&tuple).unwrap();
    assert_same_votes(&from_tuple, &entity);

    // Value roundtrip
    let value = entity.to_value(&data_type).unwrap();
    let from_value = Votes::from_value(&value).unwrap();
    assert_same_votes(&from_value, &entity);

    // Binary roundtrip
    let binary = entity.to_binary(&data_type).unwrap();
    let from_binary = Votes::from_binary(binary.as_ref()).unwrap();
    assert_same_votes(&from_binary, &entity);

    // Textual roundtrip
    let textual = entity.to_textual(&data_type).unwrap();
    let from_textual = Votes::from_textual(textual.as_str()).unwrap();
    assert_same_votes(&from_textual, &entity);

    // Clone through DatumDyn
    let cloned: Box<dyn DatumDyn> = entity.clone_boxed();
    let cloned_value = cloned.to_value(&data_type).unwrap();
    let from_cloned = Votes::from_value(&cloned_value).unwrap();
    assert_same_votes(&from_cloned, &entity);
}

#[test]
fn votes_nullable_fields_roundtrip() {
    let entity = Votes::new(
        "vote_id_val".to_string(),
        None,
        "topic_val".to_string(),
        None,
        None,
        1,
        None,
    );
    let tuple = entity.to_tuple().unwrap();
    assert!(tuple.is_null(1));
    let restored = Votes::from_tuple(&tuple).unwrap();
    assert_same_votes(&restored, &entity);

    let value = entity.to_tuple_value().unwrap();
    assert!(value.values()[1].is_null());
    let restored = Votes::from_tuple_value(&value).unwrap();
    assert_same_votes(&restored, &entity);
}
