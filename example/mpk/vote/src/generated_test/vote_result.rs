//! Unit tests for the `generated::vote_result::VoteResult` object.

use mududb::contract::database::object::Object;
use mududb::types::datum::{Datum, DatumDyn};

use crate::generated::vote_result::object::{VoteResult, columns};

fn sample_vote_result() -> VoteResult {
    VoteResult::new(
        Some("vote_id_val".to_string()),
        Some("topic_val".to_string()),
        Some(1),
        Some(2),
        Some("options_val".to_string()),
    )
}

fn assert_same_vote_result(lhs: &VoteResult, rhs: &VoteResult) {
    assert_eq!(lhs.vote_id, rhs.vote_id);
    assert_eq!(lhs.topic, rhs.topic);
    assert_eq!(lhs.vote_ended, rhs.vote_ended);
    assert_eq!(lhs.total_votes, rhs.total_votes);
    assert_eq!(lhs.options, rhs.options);
}

#[test]
fn vote_result_lifecycle() {
    let object = sample_vote_result();
    assert_eq!(object.vote_id.as_deref(), Some("vote_id_val"));
    assert_eq!(object.total_votes, Some(2));

    // Object metadata (read-only view object: no primary key statements)
    assert_eq!(VoteResult::class_name(), "vote_result");
    assert_eq!(VoteResult::TABLE_NAME, "vote_result");
    assert_eq!(VoteResult::tuple_desc().fields().len(), 5);
    assert_eq!(
        VoteResult::tuple_desc().fields()[0].name(),
        columns::VOTE_ID
    );
    assert!(VoteResult::SQL_INSERT.contains("INSERT INTO vote_result"));

    // Datum / DatumDyn metadata
    let data_type = VoteResult::data_type();
    assert_eq!(object.type_family().unwrap(), data_type.type_family());

    // Tuple roundtrip
    let tuple = object.to_tuple().unwrap();
    let from_tuple = VoteResult::from_tuple(&tuple).unwrap();
    assert_same_vote_result(&from_tuple, &object);

    // Value roundtrip
    let value = object.to_value(&data_type).unwrap();
    let from_value = VoteResult::from_value(&value).unwrap();
    assert_same_vote_result(&from_value, &object);

    // Binary roundtrip
    let binary = object.to_binary(&data_type).unwrap();
    let from_binary = VoteResult::from_binary(binary.as_ref()).unwrap();
    assert_same_vote_result(&from_binary, &object);

    // Textual roundtrip
    let textual = object.to_textual(&data_type).unwrap();
    let from_textual = VoteResult::from_textual(textual.as_str()).unwrap();
    assert_same_vote_result(&from_textual, &object);

    // Clone through DatumDyn
    let cloned: Box<dyn DatumDyn> = object.clone_boxed();
    let cloned_value = cloned.to_value(&data_type).unwrap();
    let from_cloned = VoteResult::from_value(&cloned_value).unwrap();
    assert_same_vote_result(&from_cloned, &object);

    // Insert parameters follow the column declaration order
    let params = object.insert_params();
    assert_eq!(params.0.as_deref(), Some("vote_id_val"));
    assert_eq!(params.2, Some(1));
    assert_eq!(params.3, Some(2));
}

#[test]
fn vote_result_nullable_fields_roundtrip() {
    let object = VoteResult::new(None, None, None, None, None);
    let tuple = object.to_tuple().unwrap();
    assert!(tuple.is_null(0));
    assert!(tuple.is_null(4));
    let restored = VoteResult::from_tuple(&tuple).unwrap();
    assert_same_vote_result(&restored, &object);

    let value = object.to_tuple_value().unwrap();
    assert!(value.values()[0].is_null());
    let restored = VoteResult::from_tuple_value(&value).unwrap();
    assert_same_vote_result(&restored, &object);
}
