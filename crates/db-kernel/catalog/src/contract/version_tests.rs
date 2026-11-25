use super::epoch_oracle::EPOCH_XID_BASE;
use super::snapshot::{RunningXList, Snapshot};
use super::timestamp::Timestamp;
use super::version_delta::VersionDelta;
use super::version_tuple::VersionTuple;
use mudu::common::update_delta::UpdateDelta;

#[test]
fn tuple_payload_and_timestamp_can_change_independently() {
    let initial = Timestamp::new(10, u64::MAX);
    let mut tuple = VersionTuple::new(initial.clone(), vec![1, 2]);
    assert!(!tuple.is_deleted());
    assert_eq!(tuple.timestamp(), &initial);
    tuple.mut_tuple().push(3);
    let committed = Timestamp::new(20, 30);
    tuple.update_timestamp(committed.clone());
    assert_eq!(tuple.tuple(), &[1, 2, 3]);
    assert_eq!(tuple.clone().timestamp_into(), committed);
    assert_eq!(tuple.tuple_into(), vec![1, 2, 3]);
    let deleted = VersionTuple::new_delete(initial.clone());
    assert!(deleted.is_deleted());
    assert!(deleted.tuple().is_empty());
    assert_eq!(deleted.timestamp_into(), initial);
}

#[test]
fn version_delta_preserves_undo_order_and_delete_marker() {
    let timestamp = Timestamp::new(10, 20);
    let updates = vec![
        UpdateDelta::new(1, 2, vec![9]),
        UpdateDelta::new(0, 0, vec![7]),
    ];
    for deleted in [false, true] {
        let delta = VersionDelta::new(timestamp.clone(), deleted, updates.clone());
        assert_eq!(delta.timestamp(), &timestamp);
        assert_eq!(delta.is_deleted(), deleted);
        assert_eq!(delta.update_delta(), &updates);
        assert_eq!(delta.update_delta_into(), updates);
    }
}

#[test]
fn epoch_snapshot_respects_creation_and_deletion_boundaries() {
    let epoch = EPOCH_XID_BASE + 10;
    let running = RunningXList::new_with_epoch(42, vec![1, 5], epoch);
    assert_eq!(running.read_epoch(), Some(epoch));
    let snapshot = Snapshot::from(running);
    assert_eq!(snapshot.xid(), 42);
    for (created, deleted, visible) in [
        (1, u64::MAX, true),
        (epoch - 1, u64::MAX, true),
        (epoch, u64::MAX, true),
        (epoch + 1, u64::MAX, false),
        (epoch - 1, epoch, false),
        (epoch - 1, epoch + 1, true),
        (1, 2, false),
    ] {
        assert_eq!(
            snapshot.is_tuple_visible(&Timestamp::new(created, deleted)),
            visible
        );
    }
    let legacy = RunningXList::new(42, vec![]);
    assert_eq!(legacy.read_epoch(), None);
    let snapshot = Snapshot::from(legacy);
    assert!(snapshot.is_visible(42));
    assert!(snapshot.is_tuple_visible(&Timestamp::new(41, u64::MAX)));
    assert!(!snapshot.is_tuple_visible(&Timestamp::new(43, u64::MAX)));
}
