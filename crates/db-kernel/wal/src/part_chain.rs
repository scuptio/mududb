//! Pure multipart sequencing and byte accumulation used by WAL decoding.

/// Accepts the next remaining-part counter. None rejects a broken chain;
/// Some(None) completes an entry and Some(Some(n)) retains a pending entry.
pub(crate) fn advance(previous: Option<u32>, remaining: u32) -> Option<Option<u32>> {
    if let Some(previous) = previous {
        if previous == 0 || remaining != previous - 1 {
            return None;
        }
    }
    Some(if remaining == 0 {
        None
    } else {
        Some(remaining)
    })
}

/// Appends one payload without dropping or duplicating existing bytes.
/// Overflow is rejected before any mutation.
pub(crate) fn append(payload: &mut Vec<u8>, part: &[u8]) -> bool {
    if part.len() > usize::MAX - payload.len() {
        return false;
    }
    payload.extend_from_slice(part);
    true
}
