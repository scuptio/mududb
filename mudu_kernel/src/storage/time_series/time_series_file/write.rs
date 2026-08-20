use super::page::{build_entries_page_image, empty_page_image, page_entries_fit};
use super::plan::{PlannedPageWrite, TimeSeriesFileMutationPlan};
use super::{TimeSeriesFile, TimeSeriesRecord};
use crate::storage::page::page_block_ref::PageBlockRef;
use crate::storage::page::page_block_ref_mut::PageBlockRefMut;
use crate::storage::page::page_header::NONE_PAGE_ID;
use crate::storage::page::PageId;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_utils::scoped_task_trace;
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PageInsertLocation {
    Existing(PageId),
    /// The record is newer than everything on the head page: pack it into
    /// the head page, or seal the head and start a fresh page when full
    /// (instead of splitting and re-serializing the head page).
    HeadLatest(PageId),
    Before(PageId),
    After(PageId),
    EmptyFile,
}

/// In-memory view of the file's chain state while one batched mutation is
/// being planned. Page reads fall through to the file when the batch has not
/// touched the page yet; page writes only update the pending overlay. The
/// batch ends with a single `persist_plan`, so one multi-row commit appends
/// one PL WAL batch (one pwrite) and publishes the page-cache update once,
/// after every row has been planned against the latest in-memory images.
struct WriteOverlay<'a> {
    file: &'a TimeSeriesFile,
    page_images: HashMap<PageId, Arc<Vec<u8>>>,
    page_count: PageId,
    head_page_id: Option<PageId>,
    tail_page_id: Option<PageId>,
}

impl<'a> WriteOverlay<'a> {
    fn new(file: &'a TimeSeriesFile) -> Self {
        Self {
            file,
            page_images: HashMap::new(),
            page_count: file.page_count(),
            head_page_id: file.head_page_id(),
            tail_page_id: file.tail_page_id(),
        }
    }

    /// Hands out the next page id for a page allocated inside this batch.
    fn alloc_page_id(&mut self) -> PageId {
        let page_id = self.page_count;
        self.page_count = PageId::new(page_id.as_u64() + 1);
        page_id
    }

    /// Returns the newest image of `page_id`: the pending batch image when
    /// the batch already rewrote the page, otherwise the file's published
    /// image (page cache or data file).
    async fn read_page(&self, page_id: PageId) -> RS<Arc<Vec<u8>>> {
        if let Some(image) = self.page_images.get(&page_id) {
            return Ok(image.clone());
        }
        self.file.read_page(page_id).await
    }

    /// Records the batch's newest image of `page_id`; a later write to the
    /// same page replaces the earlier one because intermediate batch states
    /// are never published.
    fn write_page_image(&mut self, page_id: PageId, image: Vec<u8>) {
        self.page_images.insert(page_id, Arc::new(image));
    }

    /// Hands the newest image of `page_id` to a caller that is about to
    /// mutate it. When the batch already rewrote the page the pending image
    /// is moved out of the overlay — rows of one batch that land on the same
    /// page therefore mutate a single buffer instead of cloning the whole
    /// page image per row. The caller must put the image back with
    /// [`WriteOverlay::write_page_image`] on every path (including failed
    /// inserts that leave the buffer unmodified) so the pending batch state
    /// is not lost.
    async fn take_page_for_mutation(&mut self, page_id: PageId) -> RS<Vec<u8>> {
        if let Some(image) = self.page_images.remove(&page_id) {
            return Ok(Arc::try_unwrap(image).unwrap_or_else(|image| (*image).clone()));
        }
        let image = self.file.read_page(page_id).await?;
        Ok((*image).clone())
    }

    /// Folds the pending images and chain metadata into the single mutation
    /// plan persisted at the end of the batch. Only fields that differ from
    /// the file's published state become `next_*` updates.
    fn into_plan(self) -> TimeSeriesFileMutationPlan {
        let mut plan = TimeSeriesFileMutationPlan::default();
        let mut images: Vec<(PageId, Arc<Vec<u8>>)> = self.page_images.into_iter().collect();
        images.sort_by_key(|(page_id, _)| *page_id);
        for (page_id, image) in images {
            plan.page_writes.push(PlannedPageWrite {
                page_id,
                image: Arc::try_unwrap(image).unwrap_or_else(|image| (*image).clone()),
            });
        }
        if self.page_count != self.file.page_count() {
            plan.next_page_count = Some(self.page_count);
        }
        if self.head_page_id != self.file.head_page_id() {
            plan.next_head_page_id = Some(self.head_page_id);
        }
        if self.tail_page_id != self.file.tail_page_id() {
            plan.next_tail_page_id = Some(self.tail_page_id);
        }
        plan
    }
}

impl TimeSeriesFile {
    pub async fn insert(&self, timestamp: u64, tuple_id: u64, payload: &[u8]) -> RS<()> {
        self.insert_batch(&[(timestamp, tuple_id, payload)]).await
    }

    /// Inserts `rows` as one file mutation: every row is planned against the
    /// overlay carrying the previous rows' page images, then the merged plan
    /// is persisted with a single PL WAL append and applied once. Behavior
    /// per row is identical to calling [`TimeSeriesFile::insert`] for each
    /// row in order.
    pub async fn insert_batch(&self, rows: &[(u64, u64, &[u8])]) -> RS<()> {
        scoped_task_trace!();
        if rows.is_empty() {
            return Ok(());
        }
        let result = async {
            // The whole locate->plan->persist->apply sequence mutates the page
            // chain across await points; serialize writers of this file. Readers
            // never take this latch.
            let _write_guard = {
                let _stage = crate::server::stage_stats::StageGuard::new(
                    crate::server::stage_stats::Stage::WrFileLatch,
                );
                self.write_latch.lock().await
            };
            let mut overlay = WriteOverlay::new(self);
            for &(timestamp, tuple_id, payload) in rows {
                self.plan_row_insert(&mut overlay, timestamp, tuple_id, payload)
                    .await?;
            }
            self.persist_plan(overlay.into_plan()).await
        }
        .await;
        result?;
        self.flush_dirty_pages_if_over_threshold().await
    }

    /// Flushes dirty pages when the deferred-flush watermark is reached.
    /// Runs OUTSIDE `write_latch`: the flush is a memory backstop (durability
    /// comes from the WAL, and the background driver repeats it every
    /// `DIRTY_PAGE_FLUSH_INTERVAL`), and `dirty_flush_latch` plus the
    /// `Arc::ptr_eq` re-check in `flush_dirty_pages` already make it safe
    /// against concurrent publishers. Keeping it inside `write_latch`
    /// serialized every writer of this file behind a full data-file flush.
    pub(crate) async fn flush_dirty_pages_if_over_threshold(&self) -> RS<()> {
        if self.dirty_pages.len() >= super::DIRTY_PAGE_FLUSH_THRESHOLD {
            self.flush_dirty_pages().await?;
        }
        Ok(())
    }

    /// Plans the insertion of one record into `overlay`, mirroring what the
    /// single-row insert path would do against the published file state.
    async fn plan_row_insert(
        &self,
        overlay: &mut WriteOverlay<'_>,
        timestamp: u64,
        tuple_id: u64,
        payload: &[u8],
    ) -> RS<()> {
        let location = {
            let _stage = crate::server::stage_stats::StageGuard::new(
                crate::server::stage_stats::Stage::WrLocate,
            );
            self.find_insert_location(overlay, timestamp).await?
        };
        let _stage = crate::server::stage_stats::StageGuard::new(
            crate::server::stage_stats::Stage::WrPageOp,
        );
        match location {
            PageInsertLocation::EmptyFile => {
                let page_id = overlay.alloc_page_id();
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
                overlay.write_page_image(page_id, page_buf);
                overlay.head_page_id = Some(page_id);
                overlay.tail_page_id = Some(page_id);
            }
            location @ (PageInsertLocation::Existing(page_id)
            | PageInsertLocation::HeadLatest(page_id)) => {
                let update_slot_index = {
                    let page_buf = overlay.read_page(page_id).await?;
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
                    page.find_slot_index(timestamp, tuple_id)?
                };
                if let Some(slot_index) = update_slot_index {
                    Self::plan_update_in_page(
                        overlay, page_id, slot_index, timestamp, tuple_id, payload,
                    )
                    .await?;
                    return Ok(());
                }

                let mut page_buf = overlay.take_page_for_mutation(page_id).await?;
                let insert_result = {
                    let mut page_mut = PageBlockRefMut::new(&mut page_buf);
                    page_mut.insert_record(timestamp, tuple_id, payload)
                };
                match insert_result {
                    Ok(_) => overlay.write_page_image(page_id, page_buf),
                    Err(err) if err.ec() == ErrorCode::InsufficientBufferSpace => {
                        // The failed insert leaves the buffer unmodified;
                        // put it back so the pending batch state survives.
                        overlay.write_page_image(page_id, page_buf);
                        if matches!(location, PageInsertLocation::HeadLatest(_)) {
                            // Head page is full: seal it and start a fresh
                            // head page with just this record instead of
                            // splitting (which would re-serialize the whole
                            // head page on every fill).
                            self.plan_insert_before_page(
                                overlay, page_id, timestamp, tuple_id, payload,
                            )
                            .await?;
                        } else {
                            self.plan_split_insert_full_page(
                                overlay, page_id, timestamp, tuple_id, payload,
                            )
                            .await?;
                        }
                    }
                    Err(err) => {
                        overlay.write_page_image(page_id, page_buf);
                        return Err(err);
                    }
                }
            }
            PageInsertLocation::Before(next_page_id) => {
                self.plan_insert_before_page(overlay, next_page_id, timestamp, tuple_id, payload)
                    .await?;
            }
            PageInsertLocation::After(prev_page_id) => {
                let page_id = overlay.alloc_page_id();
                let prev_page_buf = overlay.read_page(prev_page_id).await?;
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

                let mut updated_prev_buf = prev_page_buf.as_ref().clone();
                {
                    let header = PageBlockRef::try_new(&updated_prev_buf)?.header()?;
                    let mut page = PageBlockRefMut::new(&mut updated_prev_buf);
                    page.set_page_links(header.prev_page(), page_id)?;
                }

                overlay.write_page_image(page_id, new_page_buf);
                overlay.write_page_image(prev_page_id, updated_prev_buf);
                if let Some(next_page_id) = next_page_id {
                    let next_page_buf = overlay.read_page(next_page_id).await?;
                    let mut updated_next_buf = next_page_buf.as_ref().clone();
                    let header = PageBlockRef::try_new(&updated_next_buf)?.header()?;
                    {
                        let mut page = PageBlockRefMut::new(&mut updated_next_buf);
                        page.set_page_links(page_id, header.next_page())?;
                    }
                    overlay.write_page_image(next_page_id, updated_next_buf);
                } else {
                    overlay.tail_page_id = Some(page_id);
                }
                if overlay.head_page_id.is_none() {
                    overlay.head_page_id = Some(page_id);
                }
            }
        }
        Ok(())
    }

    pub async fn delete(&self, timestamp: u64, tuple_id: u64) -> RS<bool> {
        let deleted = async {
            // Same cross-await chain mutation as insert; serialize writers.
            let _write_guard = self.write_latch.lock().await;
            let mut result: RS<bool> = Ok(false);
            let mut current = self.head_page_id();
            while let Some(page_id) = current {
                let page_buf = self.read_page(page_id).await?;
                let page = PageBlockRef::try_new(&page_buf)?;
                if let Some((min_ts, max_ts)) = page.timestamp_bounds()? {
                    if timestamp > max_ts {
                        break;
                    }
                    if timestamp < min_ts {
                        current = page.active_next_page()?;
                        continue;
                    }
                    if let Some(slot_index) = page.find_slot_index(timestamp, tuple_id)? {
                        let mut page_buf = page_buf.as_ref().clone();
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
                        result = Ok(true);
                        break;
                    }
                }
                current = page.active_next_page()?;
            }
            result
        }
        .await?;
        self.flush_dirty_pages_if_over_threshold().await?;
        Ok(deleted)
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

    /// Plans a record insertion into a freshly allocated page linked before
    /// `next_page_id` (updating the head pointer when there is no previous
    /// page).
    async fn plan_insert_before_page(
        &self,
        overlay: &mut WriteOverlay<'_>,
        next_page_id: PageId,
        timestamp: u64,
        tuple_id: u64,
        payload: &[u8],
    ) -> RS<()> {
        let page_id = overlay.alloc_page_id();
        let next_page_buf = overlay.read_page(next_page_id).await?;
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

        let mut updated_next_buf = next_page_buf.as_ref().clone();
        {
            let header = PageBlockRef::try_new(&updated_next_buf)?.header()?;
            let mut page = PageBlockRefMut::new(&mut updated_next_buf);
            page.set_page_links(page_id, header.next_page())?;
        }

        overlay.write_page_image(page_id, new_page_buf);
        overlay.write_page_image(next_page_id, updated_next_buf);
        if let Some(prev_page_id) = prev_page_id {
            let prev_page_buf = overlay.read_page(prev_page_id).await?;
            let mut updated_prev_buf = prev_page_buf.as_ref().clone();
            let header = PageBlockRef::try_new(&updated_prev_buf)?.header()?;
            {
                let mut page = PageBlockRefMut::new(&mut updated_prev_buf);
                page.set_page_links(header.prev_page(), page_id)?;
            }
            overlay.write_page_image(prev_page_id, updated_prev_buf);
        } else {
            overlay.head_page_id = Some(page_id);
        }
        Ok(())
    }

    async fn plan_update_in_page(
        overlay: &mut WriteOverlay<'_>,
        page_id: PageId,
        slot_index: usize,
        timestamp: u64,
        tuple_id: u64,
        payload: &[u8],
    ) -> RS<()> {
        let mut page_buf = overlay.take_page_for_mutation(page_id).await?;
        {
            let mut page_mut = PageBlockRefMut::new(&mut page_buf);
            page_mut.update_record(slot_index, timestamp, tuple_id, payload)?;
        }
        overlay.write_page_image(page_id, page_buf);
        Ok(())
    }

    async fn plan_split_insert_full_page(
        &self,
        overlay: &mut WriteOverlay<'_>,
        page_id: PageId,
        timestamp: u64,
        tuple_id: u64,
        payload: &[u8],
    ) -> RS<()> {
        let page_buf = overlay.read_page(page_id).await?;
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
        let new_page_id = overlay.alloc_page_id();
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

        overlay.write_page_image(page_id, current_page_buf);
        overlay.write_page_image(new_page_id, new_page_buf);
        if let Some(next_page_id) = old_next_page_id {
            let next_page_buf = overlay.read_page(next_page_id).await?;
            let mut updated_next_buf = next_page_buf.as_ref().clone();
            let next_header = PageBlockRef::try_new(&updated_next_buf)?.header()?;
            {
                let mut page = PageBlockRefMut::new(&mut updated_next_buf);
                page.set_page_links(new_page_id, next_header.next_page())?;
            }
            overlay.write_page_image(next_page_id, updated_next_buf);
        } else {
            overlay.tail_page_id = Some(new_page_id);
        }
        Ok(())
    }

    async fn find_insert_location(
        &self,
        overlay: &WriteOverlay<'_>,
        timestamp: u64,
    ) -> RS<PageInsertLocation> {
        scoped_task_trace!();
        let Some(mut current) = overlay.head_page_id else {
            return Ok(PageInsertLocation::EmptyFile);
        };

        let mut last_non_empty = None;
        let mut is_head = true;
        loop {
            let page_buf = overlay.read_page(current).await?;
            let page = PageBlockRef::try_new(&page_buf)?;
            if let Some((min_ts, max_ts)) = page.timestamp_bounds()? {
                last_non_empty = Some(current);
                if timestamp > max_ts {
                    if is_head {
                        // Pack newer versions into the head page itself
                        // instead of prepending one fresh page per version;
                        // when the head is full it is sealed and a fresh
                        // head page starts (no split re-serialization).
                        return Ok(PageInsertLocation::HeadLatest(current));
                    }
                    return Ok(PageInsertLocation::Before(current));
                }
                if timestamp >= min_ts {
                    return Ok(PageInsertLocation::Existing(current));
                }
            }

            is_head = false;
            match page.active_next_page()? {
                Some(next) => current = next,
                None => return Ok(PageInsertLocation::After(last_non_empty.unwrap_or(current))),
            }
        }
    }
}
