//! Fallible equality comparison.

use crate::common::result::RS;

/// Equality comparison that can fail with a [`MuduError`](crate::error::MuduError).
pub trait Equal {
    /// Returns `Ok(true)` when `self` and `other` are equal.
    fn cmp_eq(&self, other: &Self) -> RS<bool>;
}
