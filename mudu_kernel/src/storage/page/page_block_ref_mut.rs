use crate::storage::page::page_block_ref::{align_up, PageBlockRef, PAGE_SIZE, RECORD_ALIGN};
use crate::storage::page::page_header::{PageHeader, PAGE_HEADER_SIZE};
use crate::storage::page::page_tailer::{PageTailer, PAGE_TAILER_SIZE};
use crate::storage::page::record_slot::{RecordSlot, RECORD_SLOT_SIZE};
use crate::storage::page::record_slot_ref::RecordSlotRef;
use crate::storage::page::PageId;
use crate::wal::lsn::LSN;
use mudu::common::crc::crc16;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;

/// `PageBlockRefMut` keeps the same layout and ordering guarantees as `PageBlockRef`.
///
/// Record payloads are always written on 8-byte boundaries in the data area.
/// `RecordSlot::size` stores the real payload length rather than the aligned size.
///
/// Update and delete paths compact the page after the change so that the data
/// area stays contiguous and `free_bytes` remains an exact value instead of a
/// fragmented estimate.
///
/// The page LSN is mirrored in both the header and the tailer. Initialization
/// starts at LSN 1, and every logical page rewrite bumps the LSN before the
/// tailer checksum is recomputed.
pub struct PageBlockRefMut<'a> {
    page: &'a mut [u8],
}

impl<'a> PageBlockRefMut<'a> {
    pub fn new(page: &'a mut [u8]) -> Self {
        Self { page }
    }

    pub fn page(&self) -> &[u8] {
        self.page
    }

    pub fn init_empty(&mut self, page_id: PageId) -> RS<()> {
        self.init_empty_with_tuple_meta(page_id, 0, 0, 0)
    }

    pub fn init_empty_with_tuple_meta(
        &mut self,
        page_id: PageId,
        tuple_format_version: u32,
        tuple_schema_hash: u64,
        tuple_flags: u64,
    ) -> RS<()> {
        self.check_page_len()?;
        self.page.fill(0);

        let mut header = PageHeader::new(page_id);
        header.set_lsn(LSN::new(1));
        header.set_first_free_offset(PAGE_HEADER_SIZE as u32);
        header.set_last_record_offset(PAGE_HEADER_SIZE as u32);
        header.set_record_count(0);
        header.set_free_bytes((self.tailer_offset() - PAGE_HEADER_SIZE) as u32);
        header.set_tuple_format_version(tuple_format_version);
        header.set_tuple_schema_hash(tuple_schema_hash);
        header.set_tuple_flags(tuple_flags);
        header.encode(&mut self.page[..PAGE_HEADER_SIZE])?;

        let tailer_offset = self.tailer_offset();
        let mut tailer = PageTailer::default();
        tailer.set_lsn(LSN::new(1));
        tailer.refresh_checksum(self.page)?;
        tailer.encode(&mut self.page[tailer_offset..tailer_offset + PAGE_TAILER_SIZE])?;
        Ok(())
    }

    pub fn header(&self) -> RS<PageHeader> {
        PageBlockRef::try_new(self.page)?.header()
    }

    pub fn tailer(&self) -> RS<PageTailer> {
        PageBlockRef::try_new(self.page)?.tailer()
    }

    pub fn slot(&self, sorted_index: usize) -> RS<RecordSlot> {
        PageBlockRef::try_new(self.page)?.slot(sorted_index)
    }

    pub fn slot_ref(&self, sorted_index: usize) -> RS<RecordSlotRef<'_>> {
        self.check_page_len()?;
        let count = self.header()?.record_count() as usize;
        if sorted_index >= count {
            return Err(mudu_error!(
                ErrorCode::Decode,
                format!("slot index {} out of range {}", sorted_index, count)
            ));
        }

        let offset = self.slot_offset_for_sorted_index(sorted_index);
        RecordSlotRef::new(&self.page[offset..offset + RECORD_SLOT_SIZE])
    }

    pub fn record_bytes(&self, sorted_index: usize) -> RS<&[u8]> {
        let slot = self.slot_ref(sorted_index)?;
        let offset = slot.offset() as usize;
        let size = slot.size() as usize;
        let slot_start = self.slot_region_start_for_count(self.header()?.record_count() as usize);
        if offset < PAGE_HEADER_SIZE || offset + size > slot_start {
            return Err(mudu_error!(
                ErrorCode::Decode,
                format!(
                    "record range [{}, {}) overlaps page metadata or slot array",
                    offset,
                    offset + size
                )
            ));
        }
        Ok(&self.page[offset..offset + size])
    }

    pub fn set_page_links(&mut self, prev_page: PageId, next_page: PageId) -> RS<()> {
        let mut header = self.header()?;
        header.set_lsn(header.lsn().saturating_add(1));
        header.set_prev_page(prev_page);
        header.set_next_page(next_page);
        header.encode(&mut self.page[..PAGE_HEADER_SIZE])?;
        self.refresh_tailer_checksum()?;
        Ok(())
    }

    pub fn insert_record(&mut self, timestamp: u64, tuple_id: u64, payload: &[u8]) -> RS<usize> {
        self.check_page_len()?;

        let header = self.header()?;
        let count = header.record_count() as usize;
        let data_offset = align_up(header.first_free_offset() as usize, RECORD_ALIGN);
        let padding = data_offset - header.first_free_offset() as usize;
        let next_slot_start = self.slot_region_start_for_count(count + 1);
        let record_end = data_offset + payload.len();
        if record_end > next_slot_start
            || padding + payload.len() + RECORD_SLOT_SIZE > header.free_bytes() as usize
        {
            return Err(mudu_error!(
                ErrorCode::InsufficientBufferSpace,
                "page does not have enough free space"
            ));
        }

        self.page[data_offset..record_end].copy_from_slice(payload);
        let mut slot = RecordSlot::new(
            data_offset as u32,
            payload.len() as u32,
            timestamp,
            tuple_id,
        );
        slot.set_check_sum(crc16(payload));
        let insert_at = self.find_insert_position_by_key(&slot)?;
        self.insert_slot_bytes(insert_at, &slot)?;
        self.rewrite_header_after_insert(
            &header,
            count + 1,
            record_end as u32,
            data_offset as u32,
        )?;
        Ok(insert_at)
    }

    pub fn delete_record(&mut self, sorted_index: usize) -> RS<()> {
        self.check_page_len()?;
        let mut slots = self.read_all_slots()?;
        if sorted_index >= slots.len() {
            return Err(mudu_error!(
                ErrorCode::Decode,
                format!("slot index {} out of range {}", sorted_index, slots.len())
            ));
        }

        slots.remove(sorted_index);
        self.compact_with_slots(slots)
    }

    pub fn update_record(
        &mut self,
        sorted_index: usize,
        timestamp: u64,
        tuple_id: u64,
        payload: &[u8],
    ) -> RS<usize> {
        self.check_page_len()?;
        let mut slots = self.read_all_slots()?;
        if sorted_index >= slots.len() {
            return Err(mudu_error!(
                ErrorCode::Decode,
                format!("slot index {} out of range {}", sorted_index, slots.len())
            ));
        }

        slots.remove(sorted_index);
        let mut new_slot = RecordSlot::new(0, payload.len() as u32, timestamp, tuple_id);
        new_slot.set_check_sum(crc16(payload));
        let mut entries = self.materialize_entries(&slots)?;
        entries.push((new_slot, payload.to_vec()));
        let mut sorted_slots: Vec<RecordSlot> = entries.iter().map(|(slot, _)| *slot).collect();
        sorted_slots.sort_by(|left, right| left.cmp_key(right));
        let new_index = sorted_slots
            .iter()
            .position(|slot| slot.cmp_key(&new_slot).is_eq())
            .ok_or_else(|| {
                mudu_error!(ErrorCode::Encode, "updated slot is missing after sorting")
            })?;
        self.rebuild_from_entries(entries)?;
        Ok(new_index)
    }

    pub fn compact(&mut self) -> RS<()> {
        let slots = self.read_all_slots()?;
        self.compact_with_slots(slots)
    }

    fn compact_with_slots(&mut self, slots: Vec<RecordSlot>) -> RS<()> {
        let entries = self.materialize_entries(&slots)?;
        self.rebuild_from_entries(entries)?;
        Ok(())
    }

    fn rebuild_from_entries(&mut self, mut entries: Vec<(RecordSlot, Vec<u8>)>) -> RS<()> {
        let header = self.header()?;
        entries.sort_by(|(left, _), (right, _)| left.cmp_key(right));

        let tailer_offset = self.tailer_offset();
        self.page[PAGE_HEADER_SIZE..tailer_offset].fill(0);

        let mut next_data = PAGE_HEADER_SIZE;
        let mut slots = Vec::with_capacity(entries.len());
        for (mut slot, payload) in entries {
            let offset = align_up(next_data, RECORD_ALIGN);
            let record_end = offset + payload.len();
            let slot_start = self.slot_region_start_for_count(slots.len() + 1);
            if record_end > slot_start {
                return Err(mudu_error!(
                    ErrorCode::InsufficientBufferSpace,
                    "page does not have enough free space"
                ));
            }

            self.page[offset..record_end].copy_from_slice(&payload);
            slot.set_offset(offset as u32);
            slot.set_size(payload.len() as u32);
            slot.set_check_sum(crc16(&payload));
            slots.push(slot);
            next_data = record_end;
        }

        self.rewrite_header_and_slots(header.page_id(), slots, next_data as u32)?;
        Ok(())
    }

    fn rewrite_header_and_slots(
        &mut self,
        page_id: PageId,
        slots: Vec<RecordSlot>,
        first_free_offset: u32,
    ) -> RS<()> {
        let slot_region_start = self.slot_region_start_for_count(slots.len());
        let tailer_offset = self.tailer_offset();
        self.page[slot_region_start..tailer_offset].fill(0);

        for (idx, slot) in slots.iter().enumerate() {
            let offset = tailer_offset - ((idx + 1) * RECORD_SLOT_SIZE);
            slot.encode(&mut self.page[offset..offset + RECORD_SLOT_SIZE])?;
        }

        let free_bytes = slot_region_start - first_free_offset as usize;
        let last_record_offset = slots
            .iter()
            .map(|slot| slot.offset())
            .max()
            .unwrap_or(PAGE_HEADER_SIZE as u32);
        let next_lsn = self.header()?.lsn().saturating_add(1);

        let mut header = self.header()?;
        debug_assert_eq!(header.page_id(), page_id);
        header.set_lsn(next_lsn);
        header.set_record_count(slots.len() as u32);
        header.set_first_free_offset(first_free_offset);
        header.set_last_record_offset(last_record_offset);
        header.set_free_bytes(free_bytes as u32);
        header.encode(&mut self.page[..PAGE_HEADER_SIZE])?;
        self.refresh_tailer_checksum()?;
        Ok(())
    }

    fn materialize_entries(&self, slots: &[RecordSlot]) -> RS<Vec<(RecordSlot, Vec<u8>)>> {
        let page = PageBlockRef::try_new(self.page)?;
        slots
            .iter()
            .map(|slot| {
                let offset = slot.offset() as usize;
                let size = slot.size() as usize;
                let data = page.page()[offset..offset + size].to_vec();
                Ok((*slot, data))
            })
            .collect()
    }

    fn read_all_slots(&self) -> RS<Vec<RecordSlot>> {
        let page = PageBlockRef::try_new(self.page)?;
        let count = page.slot_count()?;
        (0..count).map(|idx| page.slot(idx)).collect()
    }

    fn check_page_len(&self) -> RS<()> {
        if self.page.len() < PAGE_SIZE {
            return Err(mudu_error!(
                ErrorCode::Encode,
                format!(
                    "page block requires {} bytes, got {}",
                    PAGE_SIZE,
                    self.page.len()
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

    fn slot_offset_for_sorted_index(&self, sorted_index: usize) -> usize {
        self.tailer_offset() - ((sorted_index + 1) * RECORD_SLOT_SIZE)
    }

    fn refresh_tailer_checksum(&mut self) -> RS<()> {
        let tailer_offset = self.tailer_offset();
        let mut tailer =
            PageTailer::decode(&self.page[tailer_offset..tailer_offset + PAGE_TAILER_SIZE])?;
        tailer.set_lsn(self.header()?.lsn());
        tailer.refresh_checksum(self.page)?;
        tailer.encode(&mut self.page[tailer_offset..tailer_offset + PAGE_TAILER_SIZE])?;
        Ok(())
    }

    fn find_insert_position_by_key(&self, new_slot: &RecordSlot) -> RS<usize> {
        let page = PageBlockRef::try_new(self.page)?;
        let count = page.slot_count()?;
        let mut low = 0usize;
        let mut high = count;
        while low < high {
            let mid = low + ((high - low) / 2);
            let slot = page.slot_ref(mid)?;
            if slot.cmp_key(new_slot).is_lt() {
                low = mid + 1;
            } else {
                high = mid;
            }
        }
        Ok(low)
    }

    fn insert_slot_bytes(&mut self, insert_at: usize, slot: &RecordSlot) -> RS<()> {
        let count = self.header()?.record_count() as usize;
        let old_region_start = self.slot_region_start_for_count(count);
        let new_region_start = self.slot_region_start_for_count(count + 1);
        let moved_region_end = self.tailer_offset() - (insert_at * RECORD_SLOT_SIZE);
        if insert_at < count {
            self.page
                .copy_within(old_region_start..moved_region_end, new_region_start);
        }

        let slot_offset = self.slot_offset_for_sorted_index(insert_at);
        slot.encode(&mut self.page[slot_offset..slot_offset + RECORD_SLOT_SIZE])?;
        Ok(())
    }

    fn rewrite_header_after_insert(
        &mut self,
        header: &PageHeader,
        new_count: usize,
        first_free_offset: u32,
        last_record_offset: u32,
    ) -> RS<()> {
        let mut updated = header.clone();
        updated.set_lsn(header.lsn().saturating_add(1));
        updated.set_record_count(new_count as u32);
        updated.set_first_free_offset(first_free_offset);
        updated.set_last_record_offset(last_record_offset);
        updated.set_free_bytes(
            (self.slot_region_start_for_count(new_count) - first_free_offset as usize) as u32,
        );
        updated.encode(&mut self.page[..PAGE_HEADER_SIZE])?;
        self.refresh_tailer_checksum()?;
        Ok(())
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

    use super::PageBlockRefMut;
    use crate::storage::page::page_block_ref::{PageBlockRef, PAGE_SIZE, RECORD_ALIGN};
    use crate::storage::page::PageId;

    #[test]
    fn init_empty_page_sets_layout_boundaries() {
        let mut raw = [0u8; PAGE_SIZE];
        let mut page = PageBlockRefMut::new(&mut raw);
        page.init_empty(PageId::new(7)).unwrap();

        let ro = PageBlockRef::try_new(&raw).unwrap();
        let header = ro.header().unwrap();
        assert_eq!(header.page_id(), 7);
        assert_eq!(header.lsn(), 1);
        assert_eq!(header.record_count(), 0);
        assert_eq!(header.first_free_offset() as usize, 128);
        assert_eq!(ro.tailer().unwrap().lsn(), 1);
        ro.validate_layout().unwrap();
    }

    #[test]
    fn insert_keeps_slots_sorted_and_records_aligned() {
        let mut raw = [0u8; PAGE_SIZE];
        let mut page = PageBlockRefMut::new(&mut raw);
        page.init_empty(PageId::new(1)).unwrap();

        page.insert_record(30, 1, b"ccc").unwrap();
        page.insert_record(10, 2, b"aaa").unwrap();
        page.insert_record(20, 3, b"bbb").unwrap();

        let ro = PageBlockRef::try_new(&raw).unwrap();
        ro.validate_layout().unwrap();
        assert_eq!(ro.slot(0).unwrap().timestamp(), 10);
        assert_eq!(ro.slot(1).unwrap().timestamp(), 20);
        assert_eq!(ro.slot(2).unwrap().timestamp(), 30);
        assert_eq!(ro.record_bytes(0).unwrap(), b"aaa");
        assert_eq!(ro.record_bytes(1).unwrap(), b"bbb");
        assert_eq!(ro.record_bytes(2).unwrap(), b"ccc");
        assert_eq!(ro.slot(0).unwrap().offset() as usize % RECORD_ALIGN, 0);
        assert_eq!(ro.slot(1).unwrap().offset() as usize % RECORD_ALIGN, 0);
        assert_eq!(ro.slot(2).unwrap().offset() as usize % RECORD_ALIGN, 0);
        assert_eq!(ro.header().unwrap().lsn(), 4);
        assert_eq!(ro.tailer().unwrap().lsn(), 4);
    }

    #[test]
    fn delete_and_update_keep_layout_valid() {
        let mut raw = [0u8; PAGE_SIZE];
        let mut page = PageBlockRefMut::new(&mut raw);
        page.init_empty(PageId::new(3)).unwrap();

        page.insert_record(10, 1, b"alpha").unwrap();
        page.insert_record(20, 2, b"beta").unwrap();
        page.insert_record(30, 3, b"gamma").unwrap();
        page.delete_record(1).unwrap();
        page.update_record(1, 15, 3, b"delta").unwrap();

        let ro = PageBlockRef::try_new(&raw).unwrap();
        ro.validate_layout().unwrap();
        assert_eq!(ro.slot_count().unwrap(), 2);
        assert_eq!(ro.slot(0).unwrap().timestamp(), 10);
        assert_eq!(ro.slot(1).unwrap().timestamp(), 15);
        assert_eq!(ro.record_bytes(0).unwrap(), b"alpha");
        assert_eq!(ro.record_bytes(1).unwrap(), b"delta");
        assert_eq!(ro.header().unwrap().lsn(), 6);
        assert_eq!(ro.tailer().unwrap().lsn(), 6);
    }

    #[test]
    fn validate_layout_rejects_lsn_mismatch() {
        let mut raw = [0u8; PAGE_SIZE];
        let mut page = PageBlockRefMut::new(&mut raw);
        page.init_empty(PageId::new(9)).unwrap();

        let tailer_offset = PAGE_SIZE - crate::storage::page::page_tailer::PAGE_TAILER_SIZE;
        let mut tailer = PageBlockRef::try_new(&raw).unwrap().tailer().unwrap();
        tailer.set_lsn(tailer.lsn() + 1);
        tailer.encode(&mut raw[tailer_offset..PAGE_SIZE]).unwrap();

        let err = PageBlockRef::try_new(&raw)
            .unwrap()
            .validate_layout()
            .unwrap_err();
        let msg = format!("{err:?}");
        assert!(msg.contains("page lsn mismatch"));
    }

    #[test]
    fn validate_layout_rejects_bad_tailer_checksum() {
        let mut raw = [0u8; PAGE_SIZE];
        let mut page = PageBlockRefMut::new(&mut raw);
        page.init_empty(PageId::new(5)).unwrap();
        page.insert_record(10, 1, b"abc").unwrap();

        let record_offset = PageBlockRef::try_new(&raw)
            .unwrap()
            .slot(0)
            .unwrap()
            .offset() as usize;
        raw[record_offset] ^= 0x1;
        let err = PageBlockRef::try_new(&raw)
            .unwrap()
            .validate_layout()
            .unwrap_err();
        let msg = format!("{err:?}");
        assert!(msg.contains("checksum mismatch"));
    }
}
