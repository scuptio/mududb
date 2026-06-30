use byteorder::{ByteOrder, LittleEndian};
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use std::cmp::Ordering;

pub const RECORD_SLOT_SIZE: usize = 32;

/// The low 2 bits of `flag` describe how the payload fragment should be
/// interpreted inside the logical record stream.
pub const RECORD_SLOT_FLAG_FRAGMENT_MASK: u16 = 0b11;
/// The slot contains a complete record payload.
pub const RECORD_SLOT_FLAG_COMPLETE: u16 = 0;
/// The slot contains a partial record and more fragments continue in the next page.
pub const RECORD_SLOT_FLAG_PARTIAL_CONTINUED: u16 = 1;
/// The slot contains the last fragment of an incomplete record.
pub const RECORD_SLOT_FLAG_PARTIAL_LAST: u16 = 2;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RecordSlot {
    offset: u32,
    size: u32,
    timestamp: u64,
    tuple_id: u64,
    checksum: u16,
    flag: u16,
    reserved: u32,
}

impl RecordSlot {
    pub fn new(offset: u32, size: u32, timestamp: u64, tuple_id: u64) -> Self {
        Self {
            offset,
            size,
            timestamp,
            tuple_id,
            checksum: 0,
            flag: RECORD_SLOT_FLAG_COMPLETE,
            reserved: 0,
        }
    }

    pub(crate) fn from_raw(
        offset: u32,
        size: u32,
        timestamp: u64,
        tuple_id: u64,
        check_sum: u16,
        flag: u16,
        reserved: u32,
    ) -> Self {
        Self {
            offset,
            size,
            timestamp,
            tuple_id,
            checksum: check_sum,
            flag,
            reserved,
        }
    }

    pub fn decode(input: &[u8]) -> RS<Self> {
        if input.len() < RECORD_SLOT_SIZE {
            return Err(mudu_error!(
                ErrorCode::Decode,
                format!(
                    "record slot requires {} bytes, got {}",
                    RECORD_SLOT_SIZE,
                    input.len()
                )
            ));
        }

        Ok(Self {
            offset: LittleEndian::read_u32(&input[0..4]),
            size: LittleEndian::read_u32(&input[4..8]),
            timestamp: LittleEndian::read_u64(&input[8..16]),
            tuple_id: LittleEndian::read_u64(&input[16..24]),
            checksum: LittleEndian::read_u16(&input[24..26]),
            flag: LittleEndian::read_u16(&input[26..28]),
            reserved: LittleEndian::read_u32(&input[28..32]),
        })
    }

    pub fn encode(&self, out: &mut [u8]) -> RS<()> {
        if out.len() < RECORD_SLOT_SIZE {
            return Err(mudu_error!(
                ErrorCode::Encode,
                format!(
                    "record slot encode requires {} bytes, got {}",
                    RECORD_SLOT_SIZE,
                    out.len()
                )
            ));
        }

        LittleEndian::write_u32(&mut out[0..4], self.offset);
        LittleEndian::write_u32(&mut out[4..8], self.size);
        LittleEndian::write_u64(&mut out[8..16], self.timestamp);
        LittleEndian::write_u64(&mut out[16..24], self.tuple_id);
        LittleEndian::write_u16(&mut out[24..26], self.checksum);
        LittleEndian::write_u16(&mut out[26..28], self.flag);
        LittleEndian::write_u32(&mut out[28..32], self.reserved);
        Ok(())
    }

    pub fn cmp_key(&self, other: &Self) -> Ordering {
        self.timestamp
            .cmp(&other.timestamp)
            .then_with(|| self.tuple_id.cmp(&other.tuple_id))
            .then_with(|| self.offset.cmp(&other.offset))
    }

    pub fn fragment_kind(&self) -> u16 {
        self.flag & RECORD_SLOT_FLAG_FRAGMENT_MASK
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

    pub fn offset(&self) -> u32 {
        self.offset
    }

    pub fn size(&self) -> u32 {
        self.size
    }

    pub fn timestamp(&self) -> u64 {
        self.timestamp
    }

    pub fn tuple_id(&self) -> u64 {
        self.tuple_id
    }

    pub fn check_sum(&self) -> u16 {
        self.checksum
    }

    pub fn flag(&self) -> u16 {
        self.flag
    }

    pub fn reserved(&self) -> u32 {
        self.reserved
    }

    pub fn set_offset(&mut self, offset: u32) {
        self.offset = offset;
    }

    pub fn set_size(&mut self, size: u32) {
        self.size = size;
    }

    pub fn set_timestamp(&mut self, timestamp: u64) {
        self.timestamp = timestamp;
    }

    pub fn set_tuple_id(&mut self, tuple_id: u64) {
        self.tuple_id = tuple_id;
    }

    pub fn set_check_sum(&mut self, check_sum: u16) {
        self.checksum = check_sum;
    }

    pub fn set_flag(&mut self, flag: u16) {
        self.flag = flag;
    }

    pub fn set_fragment_kind(&mut self, fragment_kind: u16) {
        self.flag = (self.flag & !RECORD_SLOT_FLAG_FRAGMENT_MASK)
            | (fragment_kind & RECORD_SLOT_FLAG_FRAGMENT_MASK);
    }

    pub fn set_reserved(&mut self, reserved: u32) {
        self.reserved = reserved;
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::todo,
        clippy::unimplemented
    )]

    use super::{
        RecordSlot, RECORD_SLOT_FLAG_COMPLETE, RECORD_SLOT_FLAG_PARTIAL_CONTINUED,
        RECORD_SLOT_FLAG_PARTIAL_LAST, RECORD_SLOT_SIZE,
    };
    use crate::storage::page::record_slot_ref::RecordSlotRef;

    #[test]
    fn record_slot_roundtrip() {
        let mut slot = RecordSlot::new(64, 23, 17, 29);
        slot.set_check_sum(31);
        slot.set_flag(37);
        slot.set_reserved(41);

        let mut encoded = [0u8; RECORD_SLOT_SIZE];
        slot.encode(&mut encoded).unwrap();
        let decoded = RecordSlot::decode(&encoded).unwrap();
        assert_eq!(decoded, slot);
    }

    #[test]
    fn record_slot_low_two_flag_bits_describe_fragment_kind() {
        let mut slot = RecordSlot::new(64, 23, 17, 29);
        slot.set_flag(0b1111_0000);
        slot.set_fragment_kind(RECORD_SLOT_FLAG_COMPLETE);
        assert!(slot.is_complete());
        assert_eq!(slot.flag(), 0b1111_0000);

        slot.set_fragment_kind(RECORD_SLOT_FLAG_PARTIAL_CONTINUED);
        assert!(slot.is_partial_continued());
        assert_eq!(slot.flag(), 0b1111_0001);

        slot.set_fragment_kind(RECORD_SLOT_FLAG_PARTIAL_LAST);
        assert!(slot.is_partial_last());
        assert_eq!(slot.flag(), 0b1111_0010);
    }

    #[test]
    fn record_slot_ref_reads_fields_without_full_decode() {
        let mut slot = RecordSlot::new(64, 23, 17, 29);
        slot.set_check_sum(31);
        slot.set_flag(37);
        slot.set_reserved(41);

        let mut encoded = [0u8; RECORD_SLOT_SIZE];
        slot.encode(&mut encoded).unwrap();
        let slot_ref = RecordSlotRef::new(&encoded).unwrap();
        assert_eq!(slot_ref.offset(), 64);
        assert_eq!(slot_ref.size(), 23);
        assert_eq!(slot_ref.timestamp(), 17);
        assert_eq!(slot_ref.tuple_id(), 29);
        assert_eq!(slot_ref.checksum(), 31);
        assert_eq!(slot_ref.flag(), 37);
        assert_eq!(slot_ref.reserved(), 41);
        assert_eq!(slot_ref.to_owned(), slot);
    }
}
