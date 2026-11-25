//! Pure per-key delta semantics. Absence, preservation, and replacement are distinct.

/// One key's change; Keep is different from deleting the existing value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Change<T> {
    Keep,
    Set(T),
    Remove,
}

/// Plans one key using equality already computed by the payload comparator.
/// `equal` is relevant only when both sides exist.
pub(super) fn diff<T>(old_present: bool, new: Option<T>, equal: bool) -> Change<T> {
    match new {
        None => {
            if old_present {
                Change::Remove
            } else {
                Change::Keep
            }
        }
        Some(value) => {
            if old_present && equal {
                Change::Keep
            } else {
                Change::Set(value)
            }
        }
    }
}

/// Applies a key's change without touching any other key.
pub(super) fn apply<T>(old: Option<T>, change: Change<T>) -> Option<T> {
    match change {
        Change::Keep => old,
        Change::Set(value) => Some(value),
        Change::Remove => None,
    }
}

/// An already-applied batch must not replay a second time.
pub(super) fn replay_needed(page_lsn: u64, batch_lsn: u64) -> bool {
    page_lsn < batch_lsn
}
