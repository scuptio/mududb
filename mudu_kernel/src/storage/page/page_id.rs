//! Logical page identifier.
//!
//! A `PageId` is a dense, zero-based index into a relation or time-series file.
//! The physical byte offset of a page is calculated as:
//!
//! ```text
//! offset = page_id.as_u64() * PAGE_SIZE
//! ```
//!
//! `PageId` is intentionally a strong newtype over `u64` rather than a plain
//! alias, so page identifiers cannot be accidentally mixed with other numeric
//! quantities such as byte offsets, LSNs, or tuple counts.
//!
//! # Invalid sentinel
//!
//! The maximum `u64` value (`u64::MAX`) is reserved as the invalid sentinel:
//!
//! - `PageId::INVALID`, `PageId::MAX`, and `PageId::invalid()` all return this
//!   sentinel value.
//! - `PageId::is_valid()` returns `false` when the value equals the sentinel.
//! - `PageId::set_invalid()` mutates the identifier to the sentinel value.
//!
//! The sentinel is used on disk in page header fields such as `prev_page` and
//! `next_page` to mean "no previous/next page" (see [`NONE_PAGE_ID`]). Because
//! valid page counts can never reach `u64::MAX`, this value is unambiguous.

use crate::define_u64_id;

define_u64_id! {
    /// A logical page identifier inside a relation or time-series file.
    ///
    /// `PageId` wraps a `u64` and is stored as a 64-bit little-endian integer
    /// in all on-disk formats. Valid identifiers start at `0` and increase by
    /// one per allocated page. The value `u64::MAX` is reserved to mean
    /// "invalid / no page".
    pub struct PageId
}
