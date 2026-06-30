use crate::storage::page::record_slot::{
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

    pub fn offset(&self) -> u32 {
        LittleEndian::read_u32(&self.slot[0..4])
    }

    pub fn size(&self) -> u32 {
        LittleEndian::read_u32(&self.slot[4..8])
    }

    pub fn timestamp(&self) -> u64 {
        LittleEndian::read_u64(&self.slot[8..16])
    }

    pub fn tuple_id(&self) -> u64 {
        LittleEndian::read_u64(&self.slot[16..24])
    }

    pub fn checksum(&self) -> u16 {
        LittleEndian::read_u16(&self.slot[24..26])
    }

    pub fn flag(&self) -> u16 {
        LittleEndian::read_u16(&self.slot[26..28])
    }

    pub fn reserved(&self) -> u32 {
        LittleEndian::read_u32(&self.slot[28..32])
    }

    pub fn fragment_kind(&self) -> u16 {
        self.flag() & RECORD_SLOT_FLAG_FRAGMENT_MASK
    }

    pub fn is_complete(&self) -> bool {
        self.fragment_kind() == RECORD_SLOT_FLAG_COMPLETE
    }

    pub fn is_partial_continued(&self) -> bool {
        self.fragment_kind() == RECORD_SLOT_FLAG_PARTIAL_CONTINUED
    }

    pub fn is_partial_last(&self) -> bool {
        self.fragment_kind() == RECORD_SLOT_FLAG_PARTIAL_LAST
    }

    pub fn cmp_key(&self, other: &RecordSlot) -> Ordering {
        self.timestamp()
            .cmp(&other.timestamp())
            .then_with(|| self.tuple_id().cmp(&other.tuple_id()))
            .then_with(|| self.offset().cmp(&other.offset()))
    }

    pub fn to_owned(&self) -> RecordSlot {
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
