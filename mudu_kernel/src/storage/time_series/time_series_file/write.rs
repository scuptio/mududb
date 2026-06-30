use super::page::{build_entries_page_image, empty_page_image, page_entries_fit};
use super::plan::{PlannedPageWrite, TimeSeriesFileMutationPlan};
use super::{TimeSeriesFile, TimeSeriesRecord};
use crate::storage::page::page_block_ref::{PageBlockRef, PAGE_SIZE};
use crate::storage::page::page_block_ref_mut::PageBlockRefMut;
use crate::storage::page::page_header::NONE_PAGE_ID;
use crate::storage::page::PageId;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_utils::scoped_task_trace;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PageInsertLocation {
    Existing(PageId),
    Before(PageId),
    After(PageId),
    EmptyFile,
}

impl TimeSeriesFile {
    pub async fn insert(&mut self, timestamp: u64, tuple_id: u64, payload: &[u8]) -> RS<()> {
        scoped_task_trace!();
        match self.find_insert_location(timestamp).await? {
            PageInsertLocation::EmptyFile => {
                let page_id = self.page_count;
                let mut page_buf = empty_page_image(
                    page_id,
                    self.tuple_format_version,
                    self.tuple_schema_hash,
                    self.tuple_flags,
                )?;
                {
                    let mut page = PageBlockRefMut::new(&mut page_buf);
                    page.insert_record(timestamp, tuple_id, payload)?;
                }
                let mut plan = TimeSeriesFileMutationPlan::default();
                plan.page_writes.push(PlannedPageWrite {
                    page_id,
                    image: page_buf,
                });
                plan.next_page_count = Some(page_id + 1);
                plan.next_head_page_id = Some(Some(page_id));
                plan.next_tail_page_id = Some(Some(page_id));
                self.persist_plan(plan).await?;
            }
            PageInsertLocation::Existing(page_id) => {
                let page_buf = self.read_page(page_id).await?;
                let page = PageBlockRef::try_new(&page_buf)?;
                if self.tuple_schema_hash != 0 {
                    let header = page.header()?;
                    if header.tuple_schema_hash() != self.tuple_schema_hash {
                        return Err(mudu_error!(
                            ErrorCode::Decode,
                            format!(
                                "tuple schema hash mismatch on page {}: expected {} got {}",
                                page_id,
                                self.tuple_schema_hash,
                                header.tuple_schema_hash()
                            )
                        ));
                    }
                }
                if let Some(slot_index) = page.find_slot_index(timestamp, tuple_id)? {
                    self.update_in_page(page_id, slot_index, timestamp, tuple_id, payload)
                        .await?;
                    return Ok(());
                }

                let mut page_buf = page_buf;
                let insert_result = {
                    let mut page_mut = PageBlockRefMut::new(&mut page_buf);
                    page_mut.insert_record(timestamp, tuple_id, payload)
                };
                match insert_result {
                    Ok(_) => self.write_page(page_id, &page_buf).await?,
                    Err(err) if err.ec() == ErrorCode::InsufficientBufferSpace => {
                        self.split_insert_full_page(page_id, timestamp, tuple_id, payload)
                            .await?;
                    }
                    Err(err) => return Err(err),
                }
            }
            PageInsertLocation::Before(next_page_id) => {
                let page_id = self.page_count;
                let next_page_buf = self.read_page(next_page_id).await?;
                let next_page = PageBlockRef::try_new(&next_page_buf)?;
                let prev_page_id = next_page.active_prev_page()?;
                let mut new_page_buf = empty_page_image(
                    page_id,
                    self.tuple_format_version,
                    self.tuple_schema_hash,
                    self.tuple_flags,
                )?;
                {
                    let mut page = PageBlockRefMut::new(&mut new_page_buf);
                    page.set_page_links(prev_page_id.unwrap_or(NONE_PAGE_ID), next_page_id)?;
                    page.insert_record(timestamp, tuple_id, payload)?;
                }

                let mut updated_next_buf = next_page_buf.clone();
                {
                    let header = PageBlockRef::try_new(&updated_next_buf)?.header()?;
                    let mut page = PageBlockRefMut::new(&mut updated_next_buf);
                    page.set_page_links(page_id, header.next_page())?;
                }

                let mut plan = TimeSeriesFileMutationPlan::default();
                plan.page_writes.push(PlannedPageWrite {
                    page_id,
                    image: new_page_buf,
                });
                plan.page_writes.push(PlannedPageWrite {
                    page_id: next_page_id,
                    image: updated_next_buf,
                });
                if let Some(prev_page_id) = prev_page_id {
                    let prev_page_buf = self.read_page(prev_page_id).await?;
                    let mut updated_prev_buf = prev_page_buf.clone();
                    let header = PageBlockRef::try_new(&updated_prev_buf)?.header()?;
                    {
                        let mut page = PageBlockRefMut::new(&mut updated_prev_buf);
                        page.set_page_links(header.prev_page(), page_id)?;
                    }
                    plan.page_writes.push(PlannedPageWrite {
                        page_id: prev_page_id,
                        image: updated_prev_buf,
                    });
                } else {
                    plan.next_head_page_id = Some(Some(page_id));
                }
                plan.next_page_count = Some(page_id + 1);
                self.persist_plan(plan).await?;
            }
            PageInsertLocation::After(prev_page_id) => {
                let page_id = self.page_count;
                let prev_page_buf = self.read_page(prev_page_id).await?;
                let prev_page = PageBlockRef::try_new(&prev_page_buf)?;
                let next_page_id = prev_page.active_next_page()?;
                let mut new_page_buf = empty_page_image(
                    page_id,
                    self.tuple_format_version,
                    self.tuple_schema_hash,
                    self.tuple_flags,
                )?;
                {
                    let mut page = PageBlockRefMut::new(&mut new_page_buf);
                    page.set_page_links(prev_page_id, next_page_id.unwrap_or(NONE_PAGE_ID))?;
                    page.insert_record(timestamp, tuple_id, payload)?;
                }

                let mut updated_prev_buf = prev_page_buf.clone();
                {
                    let header = PageBlockRef::try_new(&updated_prev_buf)?.header()?;
                    let mut page = PageBlockRefMut::new(&mut updated_prev_buf);
                    page.set_page_links(header.prev_page(), page_id)?;
                }

                let mut plan = TimeSeriesFileMutationPlan::default();
                plan.page_writes.push(PlannedPageWrite {
                    page_id,
                    image: new_page_buf,
                });
                plan.page_writes.push(PlannedPageWrite {
                    page_id: prev_page_id,
                    image: updated_prev_buf,
                });
                if let Some(next_page_id) = next_page_id {
                    let next_page_buf = self.read_page(next_page_id).await?;
                    let mut updated_next_buf = next_page_buf.clone();
                    let header = PageBlockRef::try_new(&updated_next_buf)?.header()?;
                    {
                        let mut page = PageBlockRefMut::new(&mut updated_next_buf);
                        page.set_page_links(page_id, header.next_page())?;
                    }
                    plan.page_writes.push(PlannedPageWrite {
                        page_id: next_page_id,
                        image: updated_next_buf,
                    });
                } else {
                    plan.next_tail_page_id = Some(Some(page_id));
                }
                if self.head_page_id.is_none() {
                    plan.next_head_page_id = Some(Some(page_id));
                }
                plan.next_page_count = Some(page_id + 1);
                self.persist_plan(plan).await?;
            }
        }
        Ok(())
    }

    pub async fn delete(&mut self, timestamp: u64, tuple_id: u64) -> RS<bool> {
        let mut current = self.head_page_id;
        while let Some(page_id) = current {
            let page_buf = self.read_page(page_id).await?;
            let page = PageBlockRef::try_new(&page_buf)?;
            if let Some((min_ts, max_ts)) = page.timestamp_bounds()? {
                if timestamp > max_ts {
                    return Ok(false);
                }
                if timestamp < min_ts {
                    current = page.active_next_page()?;
                    continue;
                }
                if let Some(slot_index) = page.find_slot_index(timestamp, tuple_id)? {
                    let mut page_buf = page_buf;
                    {
                        let mut page_mut = PageBlockRefMut::new(&mut page_buf);
                        page_mut.delete_record(slot_index)?;
                    }
                    let mut plan = TimeSeriesFileMutationPlan::default();
                    plan.page_writes.push(PlannedPageWrite {
                        page_id,
                        image: page_buf,
                    });
                    self.persist_plan(plan).await?;
                    return Ok(true);
                }
            }
            current = page.active_next_page()?;
        }
        Ok(false)
    }

    fn find_split_index(&self, entries: &[TimeSeriesRecord]) -> RS<usize> {
        for split_at in 1..entries.len() {
            if page_entries_fit(&entries[..split_at]) && page_entries_fit(&entries[split_at..]) {
                return Ok(split_at);
            }
        }
        Err(mudu_error!(
            ErrorCode::InsufficientBufferSpace,
            "records do not fit into two time series pages"
        ))
    }

    fn page_entries(&self, page: &PageBlockRef<'_>, page_id: PageId) -> RS<Vec<TimeSeriesRecord>> {
        let count = page.slot_count()?;
        let mut entries = Vec::with_capacity(count);
        for slot_index in 0..count {
            let slot = page.slot_ref(slot_index)?;
            entries.push(TimeSeriesRecord {
                timestamp: slot.timestamp(),
                tuple_id: slot.tuple_id(),
                payload: page.record_bytes(slot_index)?.to_vec(),
                page_id,
                slot_index,
            });
        }
        Ok(entries)
    }

    async fn update_in_page(
        &mut self,
        page_id: PageId,
        slot_index: usize,
        timestamp: u64,
        tuple_id: u64,
        payload: &[u8],
    ) -> RS<()> {
        let mut page_buf = self.read_page(page_id).await?;
        {
            let mut page_mut = PageBlockRefMut::new(&mut page_buf);
            page_mut.update_record(slot_index, timestamp, tuple_id, payload)?;
        }
        let mut plan = TimeSeriesFileMutationPlan::default();
        plan.page_writes.push(PlannedPageWrite {
            page_id,
            image: page_buf,
        });
        self.persist_plan(plan).await
    }

    async fn split_insert_full_page(
        &mut self,
        page_id: PageId,
        timestamp: u64,
        tuple_id: u64,
        payload: &[u8],
    ) -> RS<()> {
        let page_buf = self.read_page(page_id).await?;
        let page = PageBlockRef::try_new(&page_buf)?;
        let mut entries = self.page_entries(&page, page_id)?;
        entries.push(TimeSeriesRecord {
            timestamp,
            tuple_id,
            payload: payload.to_vec(),
            page_id,
            slot_index: 0,
        });
        entries.sort_by(|left, right| {
            left.timestamp
                .cmp(&right.timestamp)
                .then_with(|| left.tuple_id.cmp(&right.tuple_id))
        });

        let split_at = self.find_split_index(&entries)?;
        let lower_entries = entries[..split_at].to_vec();
        let upper_entries = entries[split_at..].to_vec();

        let header = page.header()?;
        let old_next_page_id = page.active_next_page()?;
        let new_page_id = self.page_count;
        let current_page_buf = build_entries_page_image(
            page_id,
            header.prev_page(),
            new_page_id,
            &upper_entries,
            self.tuple_format_version,
            self.tuple_schema_hash,
            self.tuple_flags,
        )?;
        let new_page_buf = build_entries_page_image(
            new_page_id,
            page_id,
            old_next_page_id.unwrap_or(NONE_PAGE_ID),
            &lower_entries,
            self.tuple_format_version,
            self.tuple_schema_hash,
            self.tuple_flags,
        )?;

        let mut plan = TimeSeriesFileMutationPlan::default();
        plan.page_writes.push(PlannedPageWrite {
            page_id,
            image: current_page_buf,
        });
        plan.page_writes.push(PlannedPageWrite {
            page_id: new_page_id,
            image: new_page_buf,
        });
        if let Some(next_page_id) = old_next_page_id {
            let next_page_buf = self.read_page(next_page_id).await?;
            let mut updated_next_buf = next_page_buf.clone();
            let next_header = PageBlockRef::try_new(&updated_next_buf)?.header()?;
            {
                let mut page = PageBlockRefMut::new(&mut updated_next_buf);
                page.set_page_links(new_page_id, next_header.next_page())?;
            }
            plan.page_writes.push(PlannedPageWrite {
                page_id: next_page_id,
                image: updated_next_buf,
            });
        } else {
            plan.next_tail_page_id = Some(Some(new_page_id));
        }
        plan.next_page_count = Some(new_page_id + 1);
        self.persist_plan(plan).await
    }

    async fn find_insert_location(&self, timestamp: u64) -> RS<PageInsertLocation> {
        scoped_task_trace!();
        let Some(mut current) = self.head_page_id else {
            return Ok(PageInsertLocation::EmptyFile);
        };

        let mut last_non_empty = None;
        loop {
            let page_buf = self.read_page(current).await?;
            let page = PageBlockRef::try_new(&page_buf)?;
            if let Some((min_ts, max_ts)) = page.timestamp_bounds()? {
                last_non_empty = Some(current);
                if timestamp > max_ts {
                    return Ok(PageInsertLocation::Before(current));
                }
                if timestamp >= min_ts {
                    return Ok(PageInsertLocation::Existing(current));
                }
            }

            match page.active_next_page()? {
                Some(next) => current = next,
                None => return Ok(PageInsertLocation::After(last_non_empty.unwrap_or(current))),
            }
        }
    }

    async fn write_page(&mut self, page_id: PageId, page: &[u8]) -> RS<()> {
        scoped_task_trace!();
        if page.len() != PAGE_SIZE {
            return Err(mudu_error!(
                ErrorCode::Encode,
                format!(
                    "page write requires {} bytes, got {}",
                    PAGE_SIZE,
                    page.len()
                )
            ));
        }
        let mut plan = TimeSeriesFileMutationPlan::default();
        plan.page_writes.push(PlannedPageWrite {
            page_id,
            image: page.to_vec(),
        });
        self.persist_plan(plan).await
    }
}
