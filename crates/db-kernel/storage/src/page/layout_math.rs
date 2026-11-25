//! Checked arithmetic shared by page readers and writers.

/// Computes the start of the slot array, rejecting metadata overlap.
pub(crate) fn slot_start(tail: usize, header: usize, count: usize) -> Option<usize> {
    if header > tail || count > (tail - header) / 32 {
        return None;
    }
    Some(tail - count * 32)
}

/// Checks a half-open payload interval before any slice is formed.
pub(crate) fn record_end(header: usize, limit: usize, offset: usize, size: usize) -> Option<usize> {
    if offset < header || offset > limit || size > limit - offset {
        return None;
    }
    Some(offset + size)
}

/// Plans an eight-byte-aligned allocation inside the available data interval.
/// The returned pair is (aligned start, exclusive end). Padding consumes space.
pub(crate) fn allocate(first: usize, limit: usize, size: usize) -> Option<(usize, usize)> {
    if first > limit {
        return None;
    }
    let padding = (8 - first % 8) % 8;
    if padding > limit - first {
        return None;
    }
    let start = first + padding;
    if size > limit - start {
        return None;
    }
    Some((start, start + size))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checked_boundaries_and_padding() {
        assert_eq!(slot_start(4096, 128, usize::MAX), None);
        assert_eq!(slot_start(127, 128, 0), None);
        assert_eq!(record_end(128, 4096, usize::MAX, 2), None);
        assert_eq!(record_end(128, 4096, 128, usize::MAX), None);
        assert_eq!(record_end(128, 4096, 4096, 0), Some(4096));
        assert_eq!(allocate(129, 140, 4), Some((136, 140)));
        assert_eq!(allocate(129, 140, 5), None);
        assert_eq!(allocate(129, 135, 0), None);
        assert_eq!(allocate(136, 136, 0), Some((136, 136)));
        assert_eq!(allocate(usize::MAX, usize::MAX, 0), None);
    }
}
