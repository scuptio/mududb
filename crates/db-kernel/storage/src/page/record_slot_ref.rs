//! Zero-copy reader for one record slot inside a page buffer.

use crate::page::record_slot::{
    RecordSlot, RECORD_SLOT_FLAG_COMPLETE, RECORD_SLOT_FLAG_FRAGMENT_MASK,
    RECORD_SLOT_FLAG_PARTIAL_CONTINUED, RECORD_SLOT_FLAG_PARTIAL_LAST, RECORD_SLOT_SIZE,
};
use byteorder::{ByteOrder, LittleEndian};
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use std::cmp::Ordering;

/// `RecordSlotRef` is a zero-copy slot reader backed by the original page bytes.
/// It is used on hot paths such as slot binary search where only a subset of
/// fields is needed and decoding the whole slot array would be wasted work.
pub struct RecordSlotRef<'a> {
    slot: &'a [u8],
}

impl<'a> RecordSlotRef<'a> {
    /// Creates a slot reader over the 32-byte on-page encoding of one slot.
    pub fn new(slot: &'a [u8]) -> RS<Self> {
        if slot.len() < RECORD_SLOT_SIZE {
            return Err(mudu_error!(
                ErrorCode::Decode,
                format!(
                    "record slot requires {} bytes, got {}",
                    RECORD_SLOT_SIZE,
                    slot.len()
                )
            ));
        }

        Ok(Self { slot })
    }

    pub(crate) fn offset(&self) -> u32 {
        LittleEndian::read_u32(&self.slot[0..4])
    }

    pub(crate) fn size(&self) -> u32 {
        LittleEndian::read_u32(&self.slot[4..8])
    }

    pub(crate) fn timestamp(&self) -> u64 {
        LittleEndian::read_u64(&self.slot[8..16])
    }

    pub(crate) fn tuple_id(&self) -> u64 {
        LittleEndian::read_u64(&self.slot[16..24])
    }

    /// Returns the CRC16 checksum of the record payload.
    pub fn checksum(&self) -> u16 {
        LittleEndian::read_u16(&self.slot[24..26])
    }

    pub(crate) fn flag(&self) -> u16 {
        LittleEndian::read_u16(&self.slot[26..28])
    }

    /// Returns the reserved field.
    pub fn reserved(&self) -> u32 {
        LittleEndian::read_u32(&self.slot[28..32])
    }

    /// Returns the fragment kind stored in the low bits of the flag word.
    pub fn fragment_kind(&self) -> u16 {
        self.flag() & RECORD_SLOT_FLAG_FRAGMENT_MASK
    }

    /// Returns whether the slot holds a complete record payload.
    pub fn is_complete(&self) -> bool {
        self.fragment_kind() == RECORD_SLOT_FLAG_COMPLETE
    }

    /// Returns whether the slot holds a record fragment that continues in the
    /// next page.
    pub fn is_partial_continued(&self) -> bool {
        self.fragment_kind() == RECORD_SLOT_FLAG_PARTIAL_CONTINUED
    }

    /// Returns whether the slot holds the last fragment of an incomplete
    /// record.
    pub fn is_partial_last(&self) -> bool {
        self.fragment_kind() == RECORD_SLOT_FLAG_PARTIAL_LAST
    }

    pub(crate) fn cmp_key(&self, other: &RecordSlot) -> Ordering {
        self.timestamp()
            .cmp(&other.timestamp())
            .then_with(|| self.tuple_id().cmp(&other.tuple_id()))
            .then_with(|| self.offset().cmp(&other.offset()))
    }

    pub(crate) fn to_owned(&self) -> RecordSlot {
        RecordSlot::from_raw(
            self.offset(),
            self.size(),
            self.timestamp(),
            self.tuple_id(),
            self.checksum(),
            self.flag(),
            self.reserved(),
        )
    }
}
