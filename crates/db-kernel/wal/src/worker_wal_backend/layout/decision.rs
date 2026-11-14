//! Pure WAL recovery and reclamation decisions, independent of filesystem effects.

/// Computes the next identifier without wrapping or reusing an exhausted value.
pub(crate) fn successor(value: u64) -> Option<u64> {
    if value == u64::MAX {
        None
    } else {
        Some(value + 1)
    }
}

/// Keeps recovery at or above the persisted GC frontier.
// Keep the extraction first-order: the match is shared with the Lean state proof.
#[allow(clippy::manual_map)]
pub(crate) fn recovery_lsn(frontier: u64, maximum: Option<u64>) -> Option<u64> {
    match maximum {
        None => Some(frontier),
        Some(value) => match successor(value) {
            None => None,
            Some(next) => Some(if next < frontier { frontier } else { next }),
        },
    }
}

/// Reuse the tail only while its validated byte length is below capacity.
pub(crate) fn reuse_tail(size: u64, capacity: u64) -> bool {
    size < capacity
}

/// The archive bound cannot authorize a backwards GC frontier.
/// None means that no further reclamation is permitted in this round.
pub(crate) fn gc_limit(requested: u64, existing: u64, archive: Option<u64>) -> Option<u64> {
    let wanted = if requested < existing {
        existing
    } else {
        requested
    };
    match archive {
        None => Some(wanted),
        Some(bound) => {
            if bound < existing {
                None
            } else {
                Some(if bound < wanted { bound } else { wanted })
            }
        }
    }
}

/// A chunk is deletable only with a known maximum strictly below the boundary.
pub(crate) fn deletable(is_tail: bool, maximum: Option<u64>, limit: u64) -> bool {
    if is_tail {
        return false;
    }
    match maximum {
        None => false,
        Some(value) => value < limit,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extreme_identifiers_and_archive_bounds() {
        assert_eq!(successor(u64::MAX), None);
        assert_eq!(successor(u64::MAX - 1), Some(u64::MAX));
        assert_eq!(recovery_lsn(42, None), Some(42));
        assert_eq!(recovery_lsn(42, Some(0)), Some(42));
        assert_eq!(recovery_lsn(42, Some(u64::MAX)), None);
        assert_eq!(gc_limit(100, 42, Some(41)), None);
        assert_eq!(gc_limit(100, 42, Some(60)), Some(60));
        assert_eq!(gc_limit(0, 42, None), Some(42));
        assert!(!deletable(false, Some(42), 42));
        assert!(deletable(false, Some(41), 42));
        assert!(!deletable(true, Some(41), 42));
        assert!(!deletable(false, None, 42));
    }
}
