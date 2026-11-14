//! Pure truncation decisions, separate from filesystem effects.

/// Identifies a cut at the start of the first frame at or above the boundary.
pub(crate) fn cut_before(frame_lsn: u64, from_lsn: u64) -> bool {
    frame_lsn >= from_lsn
}

/// A cut only removes chunks strictly after the chunk holding the boundary.
pub(crate) fn remove_chunk(sequence: u64, cut_sequence: u64) -> bool {
    sequence > cut_sequence
}

/// Computes the post-truncation append cursor before destructive I/O begins.
/// No representable successor means the cut must be rejected without changes.
pub(crate) fn reset(sequence: u64, offset: u64, capacity: u64) -> Option<(Option<u64>, u64, u64)> {
    if sequence == u64::MAX {
        return None;
    }
    let next = sequence + 1;
    Some(if offset < capacity {
        (Some(sequence), offset, next)
    } else {
        (None, 0, next)
    })
}
