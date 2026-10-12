//! Fallible conversion into a [`RS`] result.

use crate::common::result::RS;

/// Converts a value into a fallible result.
pub trait ToResult<T>: Sized {
    /// Converts `self` into `RS<T>`.
    fn to(self) -> RS<T>;
}
