//! Unit tests for the `generated::vote_history_item::VoteHistoryItem` entity.

use mududb::contract::database::entity::Entity;
use mududb::types::datum::{Datum, DatumDyn};

use crate::generated::vote_history_item::object::{VoteHistoryItem, columns};

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
    let entity = sample_vote_history_item();
    assert_eq!(entity.vote_id.as_deref(), Some("vote_id_val"));
    assert_eq!(entity.action_time, Some(1));

    // Entity metadata (read-only view entity: no primary key statements)
    assert_eq!(VoteHistoryItem::table_name(), "vote_history_item");
    assert_eq!(VoteHistoryItem::TABLE_NAME, "vote_history_item");
    assert_eq!(VoteHistoryItem::tuple_desc().fields().len(), 5);
    assert_eq!(
        VoteHistoryItem::tuple_desc().fields()[0].name(),
        columns::VOTE_ID
    );
    assert!(VoteHistoryItem::SQL_INSERT.contains("INSERT INTO vote_history_item"));

    // Datum / DatumDyn metadata
    let data_type = VoteHistoryItem::data_type();
    assert_eq!(entity.type_family().unwrap(), data_type.type_family());

    // Tuple roundtrip
    let tuple = entity.to_tuple().unwrap();
    let from_tuple = VoteHistoryItem::from_tuple(&tuple).unwrap();
    assert_same_vote_history_item(&from_tuple, &entity);

    // Value roundtrip
    let value = entity.to_value(&data_type).unwrap();
    let from_value = VoteHistoryItem::from_value(&value).unwrap();
    assert_same_vote_history_item(&from_value, &entity);

    // Binary roundtrip
    let binary = entity.to_binary(&data_type).unwrap();
    let from_binary = VoteHistoryItem::from_binary(binary.as_ref()).unwrap();
    assert_same_vote_history_item(&from_binary, &entity);

    // Textual roundtrip
    let textual = entity.to_textual(&data_type).unwrap();
    let from_textual = VoteHistoryItem::from_textual(textual.as_str()).unwrap();
    assert_same_vote_history_item(&from_textual, &entity);

    // Clone through DatumDyn
    let cloned: Box<dyn DatumDyn> = entity.clone_boxed();
    let cloned_value = cloned.to_value(&data_type).unwrap();
    let from_cloned = VoteHistoryItem::from_value(&cloned_value).unwrap();
    assert_same_vote_history_item(&from_cloned, &entity);

    // Insert parameters follow the column declaration order
    let params = entity.insert_params();
    assert_eq!(params.0.as_deref(), Some("vote_id_val"));
    assert_eq!(params.1.as_deref(), Some("topic_val"));
    assert_eq!(params.4, Some(1));
}

#[test]
fn vote_history_item_nullable_fields_roundtrip() {
    let entity = VoteHistoryItem::new(None, None, None, None, None);
    let tuple = entity.to_tuple().unwrap();
    assert!(tuple.is_null(0));
    assert!(tuple.is_null(4));
    let restored = VoteHistoryItem::from_tuple(&tuple).unwrap();
    assert_same_vote_history_item(&restored, &entity);

    let value = entity.to_tuple_value().unwrap();
    assert!(value.values()[0].is_null());
    let restored = VoteHistoryItem::from_tuple_value(&value).unwrap();
    assert_same_vote_history_item(&restored, &entity);
}
