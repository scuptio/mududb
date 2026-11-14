use super::inner::TsFileInner;
use super::TimeSeriesRecord;
use crate::page::page_block_ref::PageBlockRef;
use mudu::common::result::RS;

impl TsFileInner {
    /// Looks up the record with exactly this `timestamp` and `tuple_id`,
    /// walking the page chain from the head (newest) page.
    pub(super) async fn get(&self, timestamp: u64, tuple_id: u64) -> RS<Option<TimeSeriesRecord>> {
        self.check_open()?;
        let mut current = self.head_page_id();
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

    pub(super) async fn scan_range(&self, begin_ts: u64, end_ts: u64) -> RS<Vec<TimeSeriesRecord>> {
        self.check_open()?;
        if begin_ts > end_ts {
            return Ok(vec![]);
        }

        let mut current = self.head_page_id();
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
}
