//! Fallible ordering comparison.

use crate::common::result::RS;
use std::cmp::Ordering;

/// Ordering comparison that can fail with a [`MuduError`](crate::error::MuduError).
pub trait Order {
    /// Returns the ordering of `self` relative to `other`.
    fn cmp_ord(&self, other: &Self) -> RS<Ordering>;
}
