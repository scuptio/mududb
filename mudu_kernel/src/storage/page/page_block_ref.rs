use crate::storage::page::page_header::{
    PageHeader, NONE_PAGE_ID, PAGE_HEADER_MAGIC, PAGE_HEADER_OFF_FIRST_FREE_OFFSET,
    PAGE_HEADER_OFF_FREE_BYTES, PAGE_HEADER_OFF_LSN, PAGE_HEADER_OFF_MAGIC,
    PAGE_HEADER_OFF_NEXT_PAGE, PAGE_HEADER_OFF_PAGE_ID, PAGE_HEADER_OFF_PREV_PAGE,
    PAGE_HEADER_OFF_RECORD_COUNT, PAGE_HEADER_OFF_TUPLE_FLAGS,
    PAGE_HEADER_OFF_TUPLE_FORMAT_VERSION, PAGE_HEADER_OFF_TUPLE_SCHEMA_HASH,
    PAGE_HEADER_OFF_VERSION, PAGE_HEADER_SIZE, PAGE_HEADER_VERSION_LATEST,
};
use crate::storage::page::page_tailer::{PageTailer, PAGE_TAILER_SIZE};
use crate::storage::page::record_slot::{RecordSlot, RECORD_SLOT_SIZE};
use crate::storage::page::record_slot_ref::RecordSlotRef;
use crate::storage::page::PageId;
use byteorder::{ByteOrder, LittleEndian};
use mudu::common::crc::crc16;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;

pub const PAGE_SIZE: usize = 4096;
pub const RECORD_ALIGN: usize = 8;

pub(crate) fn align_up(value: usize, align: usize) -> usize {
    debug_assert!(align.is_power_of_two());
    (value + (align - 1)) & !(align - 1)
}

/// `PageBlock` exposes a slotted-page view with the following physical layout:
/// `header / data / slot array / tailer`.
///
/// The data area grows toward higher addresses.
/// The slot array grows toward lower addresses from the page tailer.
///
/// Slot ordering intentionally differs between logical order and address order:
/// timestamps are sorted in ascending order, but smaller timestamps are stored at
/// higher addresses and larger timestamps are stored at lower addresses.
/// This means the physical slot array is descending when scanned from low to high
/// addresses, while the logical view returned by this module is ascending.
pub struct PageBlockRef<'a> {
    page: &'a [u8],
}

impl<'a> PageBlockRef<'a> {
    pub fn new(page: &'a [u8]) -> Self {
        Self { page }
    }

    pub fn page(&self) -> &[u8] {
        self.page
    }

    pub fn header(&self) -> RS<PageHeader> {
        self.check_page_len()?;
        PageHeader::decode(&self.page[..PAGE_HEADER_SIZE])
    }

    pub fn header_magic(&self) -> RS<u32> {
        self.check_page_len()?;
        Ok(LittleEndian::read_u32(
            &self.page[PAGE_HEADER_OFF_MAGIC..PAGE_HEADER_OFF_MAGIC + 4],
        ))
    }

    pub fn header_version(&self) -> RS<u32> {
        self.check_page_len()?;
        Ok(LittleEndian::read_u32(
            &self.page[PAGE_HEADER_OFF_VERSION..PAGE_HEADER_OFF_VERSION + 4],
        ))
    }

    pub fn header_page_id(&self) -> RS<PageId> {
        self.ensure_header_layout()?;
        Ok(LittleEndian::read_u32(
            &self.page[PAGE_HEADER_OFF_PAGE_ID..PAGE_HEADER_OFF_PAGE_ID + 4],
        ))
    }

    pub fn header_prev_page(&self) -> RS<PageId> {
        self.ensure_header_layout()?;
        Ok(LittleEndian::read_u32(
            &self.page[PAGE_HEADER_OFF_PREV_PAGE..PAGE_HEADER_OFF_PREV_PAGE + 4],
        ))
    }

    pub fn header_next_page(&self) -> RS<PageId> {
        self.ensure_header_layout()?;
        Ok(LittleEndian::read_u32(
            &self.page[PAGE_HEADER_OFF_NEXT_PAGE..PAGE_HEADER_OFF_NEXT_PAGE + 4],
        ))
    }

    pub fn header_lsn(&self) -> RS<u32> {
        self.ensure_header_layout()?;
        Ok(LittleEndian::read_u32(
            &self.page[PAGE_HEADER_OFF_LSN..PAGE_HEADER_OFF_LSN + 4],
        ))
    }

    pub fn header_record_count(&self) -> RS<u32> {
        self.ensure_header_layout()?;
        Ok(LittleEndian::read_u32(
            &self.page[PAGE_HEADER_OFF_RECORD_COUNT..PAGE_HEADER_OFF_RECORD_COUNT + 4],
        ))
    }

    pub fn header_first_free_offset(&self) -> RS<u32> {
        self.ensure_header_layout()?;
        Ok(LittleEndian::read_u32(
            &self.page[PAGE_HEADER_OFF_FIRST_FREE_OFFSET..PAGE_HEADER_OFF_FIRST_FREE_OFFSET + 4],
        ))
    }

    pub fn header_free_bytes(&self) -> RS<u32> {
        self.ensure_header_layout()?;
        Ok(LittleEndian::read_u32(
            &self.page[PAGE_HEADER_OFF_FREE_BYTES..PAGE_HEADER_OFF_FREE_BYTES + 4],
        ))
    }

    pub fn header_tuple_format_version(&self) -> RS<u32> {
        self.ensure_header_layout()?;
        Ok(LittleEndian::read_u32(
            &self.page
                [PAGE_HEADER_OFF_TUPLE_FORMAT_VERSION..PAGE_HEADER_OFF_TUPLE_FORMAT_VERSION + 4],
        ))
    }

    pub fn header_tuple_flags(&self) -> RS<u32> {
        self.ensure_header_layout()?;
        Ok(LittleEndian::read_u32(
            &self.page[PAGE_HEADER_OFF_TUPLE_FLAGS..PAGE_HEADER_OFF_TUPLE_FLAGS + 4],
        ))
    }

    pub fn header_tuple_schema_hash(&self) -> RS<u64> {
        self.ensure_header_layout()?;
        Ok(LittleEndian::read_u64(
            &self.page[PAGE_HEADER_OFF_TUPLE_SCHEMA_HASH..PAGE_HEADER_OFF_TUPLE_SCHEMA_HASH + 8],
        ))
    }

    pub fn tailer(&self) -> RS<PageTailer> {
        self.check_page_len()?;
        PageTailer::decode(
            &self.page[self.tailer_offset()..self.tailer_offset() + PAGE_TAILER_SIZE],
        )
    }

    pub fn slot_count(&self) -> RS<usize> {
        Ok(self.header_record_count()? as usize)
    }

    pub fn slot(&self, sorted_index: usize) -> RS<RecordSlot> {
        Ok(self.slot_ref(sorted_index)?.to_owned())
    }

    pub fn slot_ref(&self, sorted_index: usize) -> RS<RecordSlotRef<'_>> {
        self.check_page_len()?;
        let count = self.slot_count()?;
        if sorted_index >= count {
            return Err(m_error!(
                EC::DecodeErr,
                format!("slot index {} out of range {}", sorted_index, count)
            ));
        }

        let offset = self.slot_offset(sorted_index)?;
        RecordSlotRef::new(&self.page[offset..offset + RECORD_SLOT_SIZE])
    }

    pub fn slots(&self) -> RS<Vec<RecordSlot>> {
        let count = self.slot_count()?;
        (0..count).map(|idx| self.slot(idx)).collect()
    }

    pub fn record_bytes(&self, sorted_index: usize) -> RS<&[u8]> {
        let slot = self.slot_ref(sorted_index)?;
        let offset = slot.offset() as usize;
        let size = slot.size() as usize;
        let slot_start = self.slot_array_low_bound()?;
        if offset < PAGE_HEADER_SIZE || offset + size > slot_start {
            return Err(m_error!(
                EC::DecodeErr,
                format!(
                    "record range [{}, {}) overlaps page metadata or slot array",
                    offset,
                    offset + size
                )
            ));
        }
        Ok(&self.page[offset..offset + size])
    }

    pub fn free_bytes(&self) -> RS<usize> {
        Ok(self.header_free_bytes()? as usize)
    }

    pub fn is_empty(&self) -> RS<bool> {
        Ok(self.slot_count()? == 0)
    }

    pub fn timestamp_bounds(&self) -> RS<Option<(u64, u64)>> {
        let count = self.slot_count()?;
        if count == 0 {
            return Ok(None);
        }

        let min_ts = self.slot_ref(0)?.timestamp();
        let max_ts = self.slot_ref(count - 1)?.timestamp();
        Ok(Some((min_ts, max_ts)))
    }

    pub fn find_slot_index(&self, timestamp: u64, tuple_id: u64) -> RS<Option<usize>> {
        let count = self.slot_count()?;
        if count == 0 {
            return Ok(None);
        }

        let mut low = 0usize;
        let mut high = count;
        while low < high {
            let mid = low + ((high - low) / 2);
            let mid_ts = self.slot_ref(mid)?.timestamp();
            if mid_ts < timestamp {
                low = mid + 1;
            } else {
                high = mid;
            }
        }

        let mut idx = low;
        while idx < count {
            let slot = self.slot_ref(idx)?;
            if slot.timestamp() != timestamp {
                break;
            }
            if slot.tuple_id() == tuple_id {
                return Ok(Some(idx));
            }
            idx += 1;
        }
        Ok(None)
    }

    pub fn active_prev_page(&self) -> RS<Option<PageId>> {
        let prev = self.header_prev_page()?;
        Ok((prev != NONE_PAGE_ID).then_some(prev))
    }

    pub fn active_next_page(&self) -> RS<Option<PageId>> {
        let next = self.header_next_page()?;
        Ok((next != NONE_PAGE_ID).then_some(next))
    }

    pub fn validate_layout(&self) -> RS<()> {
        let header = self.header()?;
        if header.magic() != PAGE_HEADER_MAGIC {
            return Err(m_error!(
                EC::DecodeErr,
                format!("invalid page magic {:#x}", header.magic())
            ));
        }
        if header.version() > PAGE_HEADER_VERSION_LATEST {
            return Err(m_error!(
                EC::DecodeErr,
                format!("unsupported page format version {}", header.version())
            ));
        }

        let count = header.record_count() as usize;
        let first_free = header.first_free_offset() as usize;
        let slot_start = self.slot_region_start_for_count(count);
        if first_free < PAGE_HEADER_SIZE || first_free > slot_start {
            return Err(m_error!(
                EC::DecodeErr,
                format!(
                    "invalid free region boundary: first_free_offset={}, slot_start={}",
                    first_free, slot_start
                )
            ));
        }

        let expected_free = slot_start - first_free;
        if header.free_bytes() as usize != expected_free {
            return Err(m_error!(
                EC::DecodeErr,
                format!(
                    "free_bytes mismatch: header={}, expected={}",
                    header.free_bytes(),
                    expected_free
                )
            ));
        }

        let mut prev: Option<RecordSlot> = None;
        for idx in 0..count {
            let slot = self.slot(idx)?;
            if let Some(prev_slot) = prev {
                if prev_slot.cmp_key(&slot).is_gt() {
                    return Err(m_error!(
                        EC::DecodeErr,
                        format!("slot order is not ascending at index {}", idx)
                    ));
                }
            }

            let offset = slot.offset() as usize;
            let size = slot.size() as usize;
            if !offset.is_multiple_of(RECORD_ALIGN) {
                return Err(m_error!(
                    EC::DecodeErr,
                    format!(
                        "record offset {} is not {}-byte aligned",
                        offset, RECORD_ALIGN
                    )
                ));
            }
            if offset < PAGE_HEADER_SIZE || offset + size > slot_start {
                return Err(m_error!(
                    EC::DecodeErr,
                    format!("slot {} points outside the data region", idx)
                ));
            }
            let payload = &self.page[offset..offset + size];
            let checksum = crc16(payload);
            if slot.check_sum() != checksum {
                return Err(m_error!(
                    EC::DecodeErr,
                    format!(
                        "slot {} payload checksum mismatch: stored={}, actual={}",
                        idx,
                        slot.check_sum(),
                        checksum
                    )
                ));
            }
            prev = Some(slot);
        }

        let tailer = self.tailer()?;
        if header.lsn() != tailer.lsn() {
            return Err(m_error!(
                EC::DecodeErr,
                format!(
                    "page lsn mismatch: header={}, tailer={}",
                    header.lsn(),
                    tailer.lsn()
                )
            ));
        }
        tailer.validate_checksum(self.page)?;
        Ok(())
    }

    fn check_page_len(&self) -> RS<()> {
        if self.page.len() < PAGE_SIZE {
            return Err(m_error!(
                EC::DecodeErr,
                format!(
                    "page block requires {} bytes, got {}",
                    PAGE_SIZE,
                    self.page.len()
                )
            ));
        }
        Ok(())
    }

    fn ensure_header_layout(&self) -> RS<()> {
        self.check_page_len()?;
        let magic = self.header_magic()?;
        if magic != PAGE_HEADER_MAGIC {
            return Err(m_error!(
                EC::DecodeErr,
                format!("invalid page magic {:#x}", magic)
            ));
        }
        let version = self.header_version()?;
        if version != PAGE_HEADER_VERSION_LATEST {
            return Err(m_error!(
                EC::DecodeErr,
                format!(
                    "unsupported page format version {}, expected {}",
                    version, PAGE_HEADER_VERSION_LATEST
                )
            ));
        }
        Ok(())
    }

    fn tailer_offset(&self) -> usize {
        PAGE_SIZE - PAGE_TAILER_SIZE
    }

    fn slot_region_start_for_count(&self, count: usize) -> usize {
        self.tailer_offset() - (count * RECORD_SLOT_SIZE)
    }

    fn slot_array_low_bound(&self) -> RS<usize> {
        Ok(self.slot_region_start_for_count(self.slot_count()?))
    }

    fn slot_offset(&self, sorted_index: usize) -> RS<usize> {
        let count = self.slot_count()?;
        if sorted_index >= count {
            return Err(m_error!(
                EC::DecodeErr,
                format!("slot index {} out of range {}", sorted_index, count)
            ));
        }
        Ok(self.tailer_offset() - ((sorted_index + 1) * RECORD_SLOT_SIZE))
    }
}
