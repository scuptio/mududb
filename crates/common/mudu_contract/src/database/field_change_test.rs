//! Tests for the `FieldChange` partial-update change type.

use super::field_change::FieldChange;

#[test]
fn field_change_distinguishes_unchanged_and_set() {
    let unchanged: FieldChange<u32> = FieldChange::default();
    assert!(unchanged.is_unchanged());
    assert!(!unchanged.is_set());

    let set: FieldChange<u32> = FieldChange::from(7);
    assert!(set.is_set());
    assert!(!set.is_unchanged());
    assert_eq!(set, FieldChange::Set(7));
}
