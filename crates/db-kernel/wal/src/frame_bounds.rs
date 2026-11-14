//! Pure frame bounds and a fail-stop prefix cursor, shared by WAL parsing.

/// Returns (payload end, frame end), after checking the complete frame fits.
pub(crate) fn bounds(available: usize, payload: usize) -> Option<(usize, usize)> {
    if available < 32 || payload > available - 32 {
        return None;
    }
    Some((24 + payload, 32 + payload))
}

/// A scan cursor which can never resume after rejecting a frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Prefix {
    pub offset: usize,
    pub stopped: bool,
}

impl Prefix {
    /// Accepts a nonempty validated frame that fits, or stops at the old offset.
    /// `valid` is supplied by header, version, mirrored-part and CRC validation.
    pub(crate) fn advance(self, total: usize, length: usize, valid: bool) -> Self {
        if self.stopped || !valid || length == 0 || self.offset > total {
            return Self {
                offset: self.offset,
                stopped: true,
            };
        }
        if length > total - self.offset {
            return Self {
                offset: self.offset,
                stopped: true,
            };
        }
        Self {
            offset: self.offset + length,
            stopped: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extreme_bounds_and_sticky_failure() {
        assert_eq!(bounds(31, 0), None);
        assert_eq!(bounds(32, 0), Some((24, 32)));
        assert_eq!(bounds(usize::MAX, usize::MAX), None);
        assert_eq!(
            bounds(usize::MAX, usize::MAX - 32),
            Some((usize::MAX - 8, usize::MAX))
        );
        let prefix = Prefix {
            offset: 0,
            stopped: false,
        }
        .advance(100, 32, true);
        assert_eq!(prefix.offset, 32);
        let stopped = prefix.advance(100, 33, false);
        assert_eq!(
            stopped,
            Prefix {
                offset: 32,
                stopped: true
            }
        );
        assert_eq!(stopped.advance(100, 32, true), stopped);
        assert!(prefix.advance(100, 69, true).stopped);
        assert!(prefix.advance(100, 0, true).stopped);
    }
}
