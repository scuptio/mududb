use crate::storage::page::PageId;
use crate::wal::lsn::LSN;
use byteorder::{ByteOrder, LittleEndian};
use mudu::common::result::RS;
use mudu::compat::{self, FormatKind, PAGE_CURRENT_VERSION};
use mudu::error::ErrorCode;
use mudu::mudu_error;

pub const PAGE_HEADER_SIZE: usize = 128;
pub const NONE_PAGE_ID: PageId = PageId::MAX;
/// Magic value for the ASCII tag `PAGE`.
/// As a numeric constant it is `0x5041_4745` (`'P' 'A' 'G' 'E'`).
/// Because the header is encoded in little-endian order, the on-page byte
/// sequence is `45 47 41 50`, which corresponds to `E G A P` if read byte by byte.
pub const PAGE_HEADER_MAGIC: u32 = 0x5041_4745;

// Layout v1:
// magic(u32) + version(u32)
// + page_id(u64) + prev_page(u64) + next_page(u64) + lsn(u64) + flags(u64) + tuple_flags(u64)
// + record_count(u32) + first_free_offset(u32) + free_bytes(u32) + last_record_offset(u32)
// + tuple_format_version(u32)
// + tuple_schema_hash(u64)
// + reserved bytes.
/// Current on-disk version of the page header format.
///
/// The canonical value is defined in [`mudu::compat::PAGE_CURRENT_VERSION`];
/// this module-level constant is the local source-of-truth for header encode/decode.
pub const VERSION: u32 = PAGE_CURRENT_VERSION;

const PAGE_HEADER_FIXED_SIZE: usize = 84;
const PAGE_HEADER_RESERVED_SIZE: usize = PAGE_HEADER_SIZE - PAGE_HEADER_FIXED_SIZE;

// Field offsets for the v1 page header layout.
pub const PAGE_HEADER_OFF_MAGIC: usize = 0;
pub const PAGE_HEADER_OFF_VERSION: usize = 4;
pub const PAGE_HEADER_OFF_PAGE_ID: usize = 8;
pub const PAGE_HEADER_OFF_PREV_PAGE: usize = 16;
pub const PAGE_HEADER_OFF_NEXT_PAGE: usize = 24;
pub const PAGE_HEADER_OFF_LSN: usize = 32;
pub const PAGE_HEADER_OFF_FLAGS: usize = 40;
pub const PAGE_HEADER_OFF_TUPLE_FLAGS: usize = 48;
pub const PAGE_HEADER_OFF_RECORD_COUNT: usize = 56;
pub const PAGE_HEADER_OFF_FIRST_FREE_OFFSET: usize = 60;
pub const PAGE_HEADER_OFF_FREE_BYTES: usize = 64;
pub const PAGE_HEADER_OFF_LAST_RECORD_OFFSET: usize = 68;
pub const PAGE_HEADER_OFF_TUPLE_FORMAT_VERSION: usize = 72;
pub const PAGE_HEADER_OFF_TUPLE_SCHEMA_HASH: usize = 76;
pub const PAGE_HEADER_OFF_RESERVED: usize = 84;

const U32_LEN: usize = 4;
const U64_LEN: usize = 8;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PageHeader {
    magic: u32,
    version: u32,
    page_id: PageId,
    prev_page: PageId,
    next_page: PageId,
    lsn: LSN,
    flags: u64,
    record_count: u32,
    first_free_offset: u32,
    free_bytes: u32,
    last_record_offset: u32,
    tuple_format_version: u32,
    tuple_schema_hash: u64,
    tuple_flags: u64,
    reserved: [u8; PAGE_HEADER_RESERVED_SIZE],
}

impl PageHeader {
    pub fn new(page_id: PageId) -> Self {
        Self {
            magic: PAGE_HEADER_MAGIC,
            version: VERSION,
            page_id,
            prev_page: NONE_PAGE_ID,
            next_page: NONE_PAGE_ID,
            lsn: LSN::new(0),
            flags: 0,
            record_count: 0,
            first_free_offset: PAGE_HEADER_SIZE as u32,
            free_bytes: 0,
            last_record_offset: 0,
            tuple_format_version: 0,
            tuple_schema_hash: 0,
            tuple_flags: 0,
            reserved: [0; PAGE_HEADER_RESERVED_SIZE],
        }
    }

    pub fn decode(input: &[u8]) -> RS<Self> {
        if input.len() < PAGE_HEADER_SIZE {
            return Err(mudu_error!(
                ErrorCode::Decode,
                format!(
                    "page header requires {} bytes, got {}",
                    PAGE_HEADER_SIZE,
                    input.len()
                )
            ));
        }

        let magic =
            LittleEndian::read_u32(&input[PAGE_HEADER_OFF_MAGIC..PAGE_HEADER_OFF_MAGIC + U32_LEN]);
        compat::check_magic(FormatKind::Page, magic)?;

        let version = LittleEndian::read_u32(
            &input[PAGE_HEADER_OFF_VERSION..PAGE_HEADER_OFF_VERSION + U32_LEN],
        );
        compat::check_version(FormatKind::Page, version)?;

        decode(input, magic, version)
    }

    pub fn encode(&self, out: &mut [u8]) -> RS<()> {
        if out.len() < PAGE_HEADER_SIZE {
            return Err(mudu_error!(
                ErrorCode::Encode,
                format!(
                    "page header encode requires {} bytes, got {}",
                    PAGE_HEADER_SIZE,
                    out.len()
                )
            ));
        }

        // v1 layout.
        LittleEndian::write_u32(
            &mut out[PAGE_HEADER_OFF_MAGIC..PAGE_HEADER_OFF_MAGIC + U32_LEN],
            self.magic,
        );
        LittleEndian::write_u32(
            &mut out[PAGE_HEADER_OFF_VERSION..PAGE_HEADER_OFF_VERSION + U32_LEN],
            self.version,
        );
        LittleEndian::write_u64(
            &mut out[PAGE_HEADER_OFF_PAGE_ID..PAGE_HEADER_OFF_PAGE_ID + U64_LEN],
            self.page_id.into(),
        );
        LittleEndian::write_u64(
            &mut out[PAGE_HEADER_OFF_PREV_PAGE..PAGE_HEADER_OFF_PREV_PAGE + U64_LEN],
            self.prev_page.into(),
        );
        LittleEndian::write_u64(
            &mut out[PAGE_HEADER_OFF_NEXT_PAGE..PAGE_HEADER_OFF_NEXT_PAGE + U64_LEN],
            self.next_page.into(),
        );
        LittleEndian::write_u64(
            &mut out[PAGE_HEADER_OFF_LSN..PAGE_HEADER_OFF_LSN + U64_LEN],
            self.lsn.into(),
        );
        LittleEndian::write_u64(
            &mut out[PAGE_HEADER_OFF_FLAGS..PAGE_HEADER_OFF_FLAGS + U64_LEN],
            self.flags,
        );
        LittleEndian::write_u64(
            &mut out[PAGE_HEADER_OFF_TUPLE_FLAGS..PAGE_HEADER_OFF_TUPLE_FLAGS + U64_LEN],
            self.tuple_flags,
        );
        LittleEndian::write_u32(
            &mut out[PAGE_HEADER_OFF_RECORD_COUNT..PAGE_HEADER_OFF_RECORD_COUNT + U32_LEN],
            self.record_count,
        );
        LittleEndian::write_u32(
            &mut out
                [PAGE_HEADER_OFF_FIRST_FREE_OFFSET..PAGE_HEADER_OFF_FIRST_FREE_OFFSET + U32_LEN],
            self.first_free_offset,
        );
        LittleEndian::write_u32(
            &mut out[PAGE_HEADER_OFF_FREE_BYTES..PAGE_HEADER_OFF_FREE_BYTES + U32_LEN],
            self.free_bytes,
        );
        LittleEndian::write_u32(
            &mut out
                [PAGE_HEADER_OFF_LAST_RECORD_OFFSET..PAGE_HEADER_OFF_LAST_RECORD_OFFSET + U32_LEN],
            self.last_record_offset,
        );
        LittleEndian::write_u32(
            &mut out[PAGE_HEADER_OFF_TUPLE_FORMAT_VERSION
                ..PAGE_HEADER_OFF_TUPLE_FORMAT_VERSION + U32_LEN],
            self.tuple_format_version,
        );
        LittleEndian::write_u64(
            &mut out
                [PAGE_HEADER_OFF_TUPLE_SCHEMA_HASH..PAGE_HEADER_OFF_TUPLE_SCHEMA_HASH + U64_LEN],
            self.tuple_schema_hash,
        );
        out[PAGE_HEADER_OFF_RESERVED..PAGE_HEADER_OFF_RESERVED + PAGE_HEADER_RESERVED_SIZE]
            .copy_from_slice(&self.reserved);
        Ok(())
    }

    pub fn magic(&self) -> u32 {
        self.magic
    }

    pub fn version(&self) -> u32 {
        self.version
    }

    pub fn page_id(&self) -> PageId {
        self.page_id
    }

    pub fn prev_page(&self) -> PageId {
        self.prev_page
    }

    pub fn next_page(&self) -> PageId {
        self.next_page
    }

    pub fn lsn(&self) -> LSN {
        self.lsn
    }

    pub fn flags(&self) -> u64 {
        self.flags
    }

    pub fn record_count(&self) -> u32 {
        self.record_count
    }

    pub fn first_free_offset(&self) -> u32 {
        self.first_free_offset
    }

    pub fn free_bytes(&self) -> u32 {
        self.free_bytes
    }

    pub fn last_record_offset(&self) -> u32 {
        self.last_record_offset
    }

    pub fn reserved(&self) -> &[u8; PAGE_HEADER_RESERVED_SIZE] {
        &self.reserved
    }

    pub fn tuple_format_version(&self) -> u32 {
        self.tuple_format_version
    }

    pub fn tuple_schema_hash(&self) -> u64 {
        self.tuple_schema_hash
    }

    pub fn tuple_flags(&self) -> u64 {
        self.tuple_flags
    }

    pub fn set_prev_page(&mut self, prev_page: PageId) {
        self.prev_page = prev_page;
    }

    pub fn set_next_page(&mut self, next_page: PageId) {
        self.next_page = next_page;
    }

    pub fn set_lsn(&mut self, lsn: LSN) {
        self.lsn = lsn;
    }

    pub fn set_flags(&mut self, flags: u64) {
        self.flags = flags;
    }

    pub fn set_record_count(&mut self, record_count: u32) {
        self.record_count = record_count;
    }

    pub fn set_first_free_offset(&mut self, first_free_offset: u32) {
        self.first_free_offset = first_free_offset;
    }

    pub fn set_free_bytes(&mut self, free_bytes: u32) {
        self.free_bytes = free_bytes;
    }

    pub fn set_last_record_offset(&mut self, last_record_offset: u32) {
        self.last_record_offset = last_record_offset;
    }

    pub fn reserved_mut(&mut self) -> &mut [u8; PAGE_HEADER_RESERVED_SIZE] {
        &mut self.reserved
    }

    pub fn set_tuple_format_version(&mut self, format_version: u32) {
        self.tuple_format_version = format_version;
    }

    pub fn set_tuple_schema_hash(&mut self, schema_hash: u64) {
        self.tuple_schema_hash = schema_hash;
    }

    pub fn set_tuple_flags(&mut self, flags: u64) {
        self.tuple_flags = flags;
    }
}

fn decode(input: &[u8], magic: u32, version: u32) -> RS<PageHeader> {
    let page_id =
        LittleEndian::read_u64(&input[PAGE_HEADER_OFF_PAGE_ID..PAGE_HEADER_OFF_PAGE_ID + U64_LEN]);
    let prev_page = LittleEndian::read_u64(
        &input[PAGE_HEADER_OFF_PREV_PAGE..PAGE_HEADER_OFF_PREV_PAGE + U64_LEN],
    );
    let next_page = LittleEndian::read_u64(
        &input[PAGE_HEADER_OFF_NEXT_PAGE..PAGE_HEADER_OFF_NEXT_PAGE + U64_LEN],
    );
    let lsn = LittleEndian::read_u64(&input[PAGE_HEADER_OFF_LSN..PAGE_HEADER_OFF_LSN + U64_LEN]);
    let flags =
        LittleEndian::read_u64(&input[PAGE_HEADER_OFF_FLAGS..PAGE_HEADER_OFF_FLAGS + U64_LEN]);
    let tuple_flags = LittleEndian::read_u64(
        &input[PAGE_HEADER_OFF_TUPLE_FLAGS..PAGE_HEADER_OFF_TUPLE_FLAGS + U64_LEN],
    );
    let record_count = LittleEndian::read_u32(
        &input[PAGE_HEADER_OFF_RECORD_COUNT..PAGE_HEADER_OFF_RECORD_COUNT + U32_LEN],
    );
    let first_free_offset = LittleEndian::read_u32(
        &input[PAGE_HEADER_OFF_FIRST_FREE_OFFSET..PAGE_HEADER_OFF_FIRST_FREE_OFFSET + U32_LEN],
    );
    let free_bytes = LittleEndian::read_u32(
        &input[PAGE_HEADER_OFF_FREE_BYTES..PAGE_HEADER_OFF_FREE_BYTES + U32_LEN],
    );
    let last_record_offset = LittleEndian::read_u32(
        &input[PAGE_HEADER_OFF_LAST_RECORD_OFFSET..PAGE_HEADER_OFF_LAST_RECORD_OFFSET + U32_LEN],
    );
    let tuple_format_version = LittleEndian::read_u32(
        &input
            [PAGE_HEADER_OFF_TUPLE_FORMAT_VERSION..PAGE_HEADER_OFF_TUPLE_FORMAT_VERSION + U32_LEN],
    );
    let tuple_schema_hash = LittleEndian::read_u64(
        &input[PAGE_HEADER_OFF_TUPLE_SCHEMA_HASH..PAGE_HEADER_OFF_TUPLE_SCHEMA_HASH + U64_LEN],
    );
    let mut reserved = [0u8; PAGE_HEADER_RESERVED_SIZE];
    reserved.copy_from_slice(
        &input[PAGE_HEADER_OFF_RESERVED..PAGE_HEADER_OFF_RESERVED + PAGE_HEADER_RESERVED_SIZE],
    );
    Ok(PageHeader {
        magic,
        version,
        page_id: page_id.into(),
        prev_page: prev_page.into(),
        next_page: next_page.into(),
        lsn: lsn.into(),
        flags,
        record_count,
        first_free_offset,
        free_bytes,
        last_record_offset,
        tuple_format_version,
        tuple_schema_hash,
        tuple_flags,
        reserved,
    })
}

impl Default for PageHeader {
    fn default() -> Self {
        Self::new(PageId::new(0))
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

    use super::*;

    #[test]
    fn page_header_encodes_to_fixed_128_bytes() {
        let header = PageHeader::default();
        let mut encoded = [0u8; PAGE_HEADER_SIZE];
        header.encode(&mut encoded).unwrap();
        assert_eq!(encoded.len(), PAGE_HEADER_SIZE);
        assert_eq!(header.magic(), PAGE_HEADER_MAGIC);
        assert_eq!(header.version(), VERSION);
    }

    #[test]
    fn page_header_roundtrip() {
        let mut header = PageHeader::new(PageId::new(7));
        header.set_prev_page(PageId::new(5));
        header.set_next_page(PageId::new(9));
        header.set_lsn(LSN::new(11));
        header.set_flags(17);
        header.set_record_count(19);
        header.set_first_free_offset(23);
        header.set_free_bytes(29);
        header.set_last_record_offset(31);
        header.set_tuple_format_version(1);
        header.set_tuple_schema_hash(0xdead_beef);
        header.set_tuple_flags(7);
        header.reserved_mut()[0] = 37;

        let mut encoded = [0u8; PAGE_HEADER_SIZE];
        header.encode(&mut encoded).unwrap();
        let decoded = PageHeader::decode(&encoded).unwrap();
        assert_eq!(decoded, header);
    }
}
