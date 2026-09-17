//! Log sequence number.

use crate::define_u64_id;

define_u64_id! {
    /// A monotonic log sequence number.
    ///
    /// `LSN` is a strongly typed 64-bit identifier. It wraps a `u64` and
    /// supports the usual comparisons, saturating arithmetic, and conversions.
    pub struct LSN
}
