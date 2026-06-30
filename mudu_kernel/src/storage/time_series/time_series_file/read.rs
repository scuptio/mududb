use super::io::{page_offset, read_file_exact};
use super::{TimeSeriesFile, TimeSeriesRecord};
use crate::storage::page::page_block_ref::{PageBlockRef, PAGE_SIZE};
use crate::storage::page::PageId;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_utils::scoped_task_trace;

impl TimeSeriesFile {
    pub async fn get(&self, timestamp: u64, tuple_id: u64) -> RS<Option<TimeSeriesRecord>> {
        let mut current = self.head_page_id;
        while let Some(page_id) = current {
            let page_buf = self.read_page(page_id).await?;
            let page = PageBlockRef::try_new(&page_buf)?;
            if let Some((min_ts, max_ts)) = page.timestamp_bounds()? {
                if timestamp > max_ts {
                    return Ok(None);
                }
                if timestamp < min_ts {
                    current = page.active_next_page()?;
                    continue;
                }
                if let Some(slot_index) = page.find_slot_index(timestamp, tuple_id)? {
                    return Ok(Some(TimeSeriesRecord {
                        timestamp,
                        tuple_id,
                        payload: page.record_bytes(slot_index)?.to_vec(),
                        page_id,
                        slot_index,
                    }));
                }
            }
            current = page.active_next_page()?;
        }
        Ok(None)
    }

    pub async fn scan_range(&self, begin_ts: u64, end_ts: u64) -> RS<Vec<TimeSeriesRecord>> {
        if begin_ts > end_ts {
            return Ok(vec![]);
        }

        let mut current = self.head_page_id;
        let mut rows = vec![];
        while let Some(page_id) = current {
            let page_buf = self.read_page(page_id).await?;
            let page = PageBlockRef::try_new(&page_buf)?;
            if let Some((min_ts, max_ts)) = page.timestamp_bounds()? {
                if max_ts < begin_ts {
                    break;
                }
                if min_ts <= end_ts && max_ts >= begin_ts {
                    let count = page.slot_count()?;
                    for slot_index in 0..count {
                        let slot = page.slot_ref(slot_index)?;
                        let ts = slot.timestamp();
                        if ts < begin_ts || ts > end_ts {
                            continue;
                        }
                        rows.push(TimeSeriesRecord {
                            timestamp: ts,
                            tuple_id: slot.tuple_id(),
                            payload: page.record_bytes(slot_index)?.to_vec(),
                            page_id,
                            slot_index,
                        });
                    }
                }
            }
            current = page.active_next_page()?;
        }

        rows.sort_by(|left, right| {
            left.timestamp
                .cmp(&right.timestamp)
                .then_with(|| left.tuple_id.cmp(&right.tuple_id))
        });
        Ok(rows)
    }

    pub(super) async fn read_page(&self, page_id: PageId) -> RS<Vec<u8>> {
        scoped_task_trace!();
        if page_id >= self.page_count {
            return Err(mudu_error!(
                ErrorCode::IndexOutOfRange,
                format!("page {} out of range {}", page_id, self.page_count)
            ));
        }
        if let Some(entry) = self.page_cache.get_sync(&page_id) {
            return Ok(entry.get().clone());
        }

        let page = read_file_exact(self.file_ref()?, PAGE_SIZE, page_offset(page_id)?).await?;
        let _ = self.page_cache.remove_sync(&page_id);
        let _ = self.page_cache.insert_sync(page_id, page.clone());
        Ok(page)
    }
}
