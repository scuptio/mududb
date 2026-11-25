//! Pure on-page slot representation, codec, and field updates.

use std::cmp::Ordering;

pub(crate) const RECORD_SLOT_SIZE: usize = 32;

/// The low 2 bits of `flag` describe how the payload fragment should be
/// interpreted inside the logical record stream.
pub const RECORD_SLOT_FLAG_FRAGMENT_MASK: u16 = 0b11;
/// The slot contains a complete record payload.
pub(crate) const RECORD_SLOT_FLAG_COMPLETE: u16 = 0;
/// The slot contains a partial record and more fragments continue in the next page.
pub const RECORD_SLOT_FLAG_PARTIAL_CONTINUED: u16 = 1;
/// The slot contains the last fragment of an incomplete record.
pub const RECORD_SLOT_FLAG_PARTIAL_LAST: u16 = 2;

/// One entry of a page's slot array: where a record's payload lives in the
/// data area (`offset`/`size`), its sort key (`timestamp`, `tuple_id`), the
/// payload checksum, and the fragment flag.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StructSlot {
    offset: u32,
    size: u32,
    timestamp: u64,
    tuple_id: u64,
    checksum: u16,
    flag: u16,
    reserved: u32,
}

impl StructSlot {
    /// Creates a slot for a complete record at `offset` with payload length
    /// `size`, keyed by `timestamp` and `tuple_id`.
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

    /// Decodes the first 32 bytes without interpreting reserved bits.
    pub(crate) fn decode_bytes(input: &[u8]) -> Option<Self> {
        if input.len() < RECORD_SLOT_SIZE {
            return None;
        }
        Some(Self {
            offset: u32::from_le_bytes([input[0], input[1], input[2], input[3]]),
            size: u32::from_le_bytes([input[4], input[5], input[6], input[7]]),
            timestamp: u64::from_le_bytes([
                input[8], input[9], input[10], input[11], input[12], input[13], input[14],
                input[15],
            ]),
            tuple_id: u64::from_le_bytes([
                input[16], input[17], input[18], input[19], input[20], input[21], input[22],
                input[23],
            ]),
            checksum: u16::from_le_bytes([input[24], input[25]]),
            flag: u16::from_le_bytes([input[26], input[27]]),
            reserved: u32::from_le_bytes([input[28], input[29], input[30], input[31]]),
        })
    }

    /// Writes exactly 32 bytes; short buffers remain unchanged.
    pub(crate) fn encode_bytes(&self, out: &mut [u8]) -> bool {
        if out.len() < RECORD_SLOT_SIZE {
            return false;
        }
        out[0..4].copy_from_slice(&self.offset.to_le_bytes());
        out[4..8].copy_from_slice(&self.size.to_le_bytes());
        out[8..16].copy_from_slice(&self.timestamp.to_le_bytes());
        out[16..24].copy_from_slice(&self.tuple_id.to_le_bytes());
        out[24..26].copy_from_slice(&self.checksum.to_le_bytes());
        out[26..28].copy_from_slice(&self.flag.to_le_bytes());
        out[28..32].copy_from_slice(&self.reserved.to_le_bytes());
        true
    }

    pub(crate) fn cmp_key(&self, other: &Self) -> Ordering {
        self.timestamp
            .cmp(&other.timestamp)
            .then_with(|| self.tuple_id.cmp(&other.tuple_id))
            .then_with(|| self.offset.cmp(&other.offset))
    }

    /// Returns the fragment kind stored in the low bits of the flag word.
    pub fn fragment_kind(&self) -> u16 {
        self.flag & RECORD_SLOT_FLAG_FRAGMENT_MASK
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

    pub(crate) fn offset(&self) -> u32 {
        self.offset
    }

    pub(crate) fn size(&self) -> u32 {
        self.size
    }

    pub(crate) fn timestamp(&self) -> u64 {
        self.timestamp
    }

    pub(crate) fn tuple_id(&self) -> u64 {
        self.tuple_id
    }

    pub(crate) fn check_sum(&self) -> u16 {
        self.checksum
    }

    /// Returns the raw flag word.
    pub fn flag(&self) -> u16 {
        self.flag
    }

    /// Returns the reserved field.
    pub fn reserved(&self) -> u32 {
        self.reserved
    }

    pub(crate) fn set_offset(&mut self, offset: u32) {
        self.offset = offset;
    }

    pub(crate) fn set_size(&mut self, size: u32) {
        self.size = size;
    }

    /// Sets the record's timestamp key.
    pub fn set_timestamp(&mut self, timestamp: u64) {
        self.timestamp = timestamp;
    }

    /// Sets the record's tuple id key.
    pub fn set_tuple_id(&mut self, tuple_id: u64) {
        self.tuple_id = tuple_id;
    }

    pub(crate) fn set_check_sum(&mut self, check_sum: u16) {
        self.checksum = check_sum;
    }

    /// Sets the raw flag word.
    pub fn set_flag(&mut self, flag: u16) {
        self.flag = flag;
    }

    /// Sets the fragment-kind bits of the flag word, leaving the other bits
    /// untouched.
    pub fn set_fragment_kind(&mut self, fragment_kind: u16) {
        self.flag = (self.flag & !RECORD_SLOT_FLAG_FRAGMENT_MASK)
            | (fragment_kind & RECORD_SLOT_FLAG_FRAGMENT_MASK);
    }

    /// Sets the reserved field.
    pub fn set_reserved(&mut self, reserved: u32) {
        self.reserved = reserved;
    }
}
