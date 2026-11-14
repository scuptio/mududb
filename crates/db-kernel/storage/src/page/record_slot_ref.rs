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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use byteorder::{ByteOrder, LittleEndian};

    fn sample_slot(flag: u16) -> [u8; 32] {
        let mut raw = [0u8; RECORD_SLOT_SIZE];
        LittleEndian::write_u32(&mut raw[0..4], 100);
        LittleEndian::write_u32(&mut raw[4..8], 64);
        LittleEndian::write_u64(&mut raw[8..16], 1000);
        LittleEndian::write_u64(&mut raw[16..24], 7);
        LittleEndian::write_u16(&mut raw[24..26], 0xBEEF);
        LittleEndian::write_u16(&mut raw[26..28], flag);
        LittleEndian::write_u32(&mut raw[28..32], 9);
        raw
    }

    #[test]
    fn slot_ref_reads_every_field_and_fragment_kinds() {
        let raw = sample_slot(RECORD_SLOT_FLAG_COMPLETE);
        let slot = RecordSlotRef::new(&raw).unwrap();
        assert_eq!(slot.offset(), 100);
        assert_eq!(slot.size(), 64);
        assert_eq!(slot.timestamp(), 1000);
        assert_eq!(slot.tuple_id(), 7);
        assert_eq!(slot.checksum(), 0xBEEF);
        assert_eq!(slot.flag(), RECORD_SLOT_FLAG_COMPLETE);
        assert_eq!(slot.reserved(), 9);
        assert_eq!(slot.fragment_kind(), RECORD_SLOT_FLAG_COMPLETE);
        assert!(slot.is_complete());
        assert!(!slot.is_partial_continued());
        assert!(!slot.is_partial_last());

        let continued_raw = sample_slot(RECORD_SLOT_FLAG_PARTIAL_CONTINUED);
        let continued = RecordSlotRef::new(&continued_raw).unwrap();
        assert!(continued.is_partial_continued());
        assert!(!continued.is_complete());
        let last_raw = sample_slot(RECORD_SLOT_FLAG_PARTIAL_LAST);
        let last = RecordSlotRef::new(&last_raw).unwrap();
        assert!(last.is_partial_last());

        // Short buffers are rejected.
        assert!(RecordSlotRef::new(&raw[..16]).is_err());

        // cmp_key orders by timestamp, then tuple id, then offset.
        let owned = slot.to_owned();
        assert_eq!(slot.cmp_key(&owned), Ordering::Equal);
        let mut other_raw = sample_slot(RECORD_SLOT_FLAG_COMPLETE);
        LittleEndian::write_u64(&mut other_raw[16..24], 8);
        let other = RecordSlotRef::new(&other_raw).unwrap();
        assert_eq!(slot.cmp_key(&other.to_owned()), Ordering::Less);
    }
}
