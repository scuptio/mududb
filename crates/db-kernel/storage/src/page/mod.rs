//! Slotted-page primitives: the header and tailer codec, record slots, and
//! the read-only and mutable page-block views over a page buffer.

pub mod format;
pub mod migrate;
pub mod page_block_ref;
pub mod page_block_ref_mut;
#[cfg(test)]
mod page_block_ref_test;
pub mod page_header;
pub mod page_tailer;
pub mod record_slot;
pub mod record_slot_ref;

pub use ::mudu::common::page_id::PageId;

mod layout_math;
mod record_slot_core;

// The verified arithmetic models the current disk slot width and record alignment.
// A format change must update the pure functions and their proofs together.
const _: () = assert!(record_slot::RECORD_SLOT_SIZE == 32 && page_block_ref::RECORD_ALIGN == 8);

pub(crate) mod sequence_edit;

#[cfg(test)]
mod sequence_edit_tests;
