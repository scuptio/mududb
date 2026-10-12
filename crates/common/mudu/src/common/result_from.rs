//! Fallible conversion from another type into `Self`.

use crate::common::result::RS;

/// Fallible conversion from `T` into `Self`.
pub trait ResultFrom<T>: Sized {
    /// Attempts to convert `value` into `Self`.
    fn from(value: T) -> RS<Self>;
}
