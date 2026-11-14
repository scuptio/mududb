//! Pure sequence edits used by page insert, delete, update, and rebuild paths.

/// Replaces a half-open range, preserving the prefix and suffix exactly.
/// Invalid bounds or an unrepresentable result length leave the input intact.
pub(crate) fn splice<T: Clone>(
    items: &[T],
    start: usize,
    end: usize,
    replacement: &[T],
) -> Option<Vec<T>> {
    if start > end || end > items.len() {
        return None;
    }
    let retained = items.len() - (end - start);
    if replacement.len() > usize::MAX - retained {
        return None;
    }
    let mut output = Vec::new();
    output.extend_from_slice(&items[..start]);
    output.extend_from_slice(replacement);
    output.extend_from_slice(&items[end..]);
    Some(output)
}

/// Publishes a prepared page image in one infallible copy after all checks.
/// A rejected image cannot partially overwrite the destination.
pub(crate) fn publish(page: &mut [u8], prepared: &[u8]) -> bool {
    if page.len() != prepared.len() {
        return false;
    }
    page.copy_from_slice(prepared);
    true
}

/// The shared strict ordering of page and delta record keys.
pub(crate) fn key_less(left_time: u64, left_id: u64, right_time: u64, right_id: u64) -> bool {
    left_time < right_time || (left_time == right_time && left_id < right_id)
}

/// Relocates one materialized payload without changing bytes outside its range.
/// The caller supplies the data-region slice, excluding the slot array.
pub(crate) fn copy_payload(page: &mut [u8], offset: usize, source: &[u8]) -> bool {
    if offset > page.len() || source.len() > page.len() - offset {
        return false;
    }
    let end = offset + source.len();
    page[offset..end].copy_from_slice(source);
    true
}
