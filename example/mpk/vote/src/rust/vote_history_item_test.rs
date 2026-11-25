//! Unit tests for the `rust::vote_history_item::VoteHistoryItem` object.

use mududb::contract::database::object::Object;
use mududb::types::datum::{Datum, DatumDyn};

use crate::rust::vote_history_item::object::{VoteHistoryItem, columns};

fn sample_vote_history_item() -> VoteHistoryItem {
    VoteHistoryItem::new(
        Some("vote_id_val".to_string()),
        Some("topic_val".to_string()),
        Some(1),
        Some(0),
        Some(1),
    )
}

fn assert_same_vote_history_item(lhs: &VoteHistoryItem, rhs: &VoteHistoryItem) {
    assert_eq!(lhs.vote_id, rhs.vote_id);
    assert_eq!(lhs.topic, rhs.topic);
    assert_eq!(lhs.action_time, rhs.action_time);
    assert_eq!(lhs.is_withdrawn, rhs.is_withdrawn);
    assert_eq!(lhs.vote_ended, rhs.vote_ended);
}

#[test]
fn vote_history_item_lifecycle() {
    let object = sample_vote_history_item();
    assert_eq!(object.vote_id.as_deref(), Some("vote_id_val"));
    assert_eq!(object.action_time, Some(1));

    // Object metadata (read-only view object: no primary key statements)
    assert_eq!(VoteHistoryItem::class_name(), "vote_history_item");
    assert_eq!(VoteHistoryItem::TABLE_NAME, "vote_history_item");
    assert_eq!(VoteHistoryItem::tuple_desc().fields().len(), 5);
    assert_eq!(
        VoteHistoryItem::tuple_desc().fields()[0].name(),
        columns::VOTE_ID
    );
    assert!(VoteHistoryItem::SQL_INSERT.contains("INSERT INTO vote_history_item"));

    // Datum / DatumDyn metadata
    let data_type = VoteHistoryItem::data_type();
    assert_eq!(object.type_family().unwrap(), data_type.type_family());

    // Tuple roundtrip
    let tuple = object.to_tuple().unwrap();
    let from_tuple = VoteHistoryItem::from_tuple(&tuple).unwrap();
    assert_same_vote_history_item(&from_tuple, &object);

    // Value roundtrip
    let value = object.to_value(&data_type).unwrap();
    let from_value = VoteHistoryItem::from_value(&value).unwrap();
    assert_same_vote_history_item(&from_value, &object);

    // Binary roundtrip
    let binary = object.to_binary(&data_type).unwrap();
    let from_binary = VoteHistoryItem::from_binary(binary.as_ref()).unwrap();
    assert_same_vote_history_item(&from_binary, &object);

    // Textual roundtrip
    let textual = object.to_textual(&data_type).unwrap();
    let from_textual = VoteHistoryItem::from_textual(textual.as_str()).unwrap();
    assert_same_vote_history_item(&from_textual, &object);

    // Clone through DatumDyn
    let cloned: Box<dyn DatumDyn> = object.clone_boxed();
    let cloned_value = cloned.to_value(&data_type).unwrap();
    let from_cloned = VoteHistoryItem::from_value(&cloned_value).unwrap();
    assert_same_vote_history_item(&from_cloned, &object);

    // Insert parameters follow the column declaration order
    let params = object.insert_params();
    assert_eq!(params.0.as_deref(), Some("vote_id_val"));
    assert_eq!(params.1.as_deref(), Some("topic_val"));
    assert_eq!(params.4, Some(1));
}

#[test]
fn vote_history_item_nullable_fields_roundtrip() {
    let object = VoteHistoryItem::new(None, None, None, None, None);
    let tuple = object.to_tuple().unwrap();
    assert!(tuple.is_null(0));
    assert!(tuple.is_null(4));
    let restored = VoteHistoryItem::from_tuple(&tuple).unwrap();
    assert_same_vote_history_item(&restored, &object);

    let value = object.to_tuple_value().unwrap();
    assert!(value.values()[0].is_null());
    let restored = VoteHistoryItem::from_tuple_value(&value).unwrap();
    assert_same_vote_history_item(&restored, &object);
}
