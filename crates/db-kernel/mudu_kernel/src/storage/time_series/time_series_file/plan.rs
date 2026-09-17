use super::io::{
    close_file, ensure_time_series_file_exists_async, ensure_time_series_file_exists_async_no_fs,
    remove_file_if_exists_async,
};
use super::page::{finalize_page_image_checksum, stamp_page_image_lsn};
use super::{raw_page_id, TimeSeriesFile};
use crate::storage::page::page_block_ref::{PageBlockRef, PAGE_SIZE};
use crate::storage::page::PageId;
use crate::wal::log_frame::frame_lsns;
use crate::wal::pl_batch::PLBatch;
use crate::wal::pl_entry::{
    PLEntry, PLFileId, PLOp, PLPageInit, PLPageLinks, PLRecord, PLRecordKey, PageDelta,
};
use crate::wal::worker_log::WorkerLogBackend;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_utils::scoped_task_trace;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tracing::trace;

#[derive(Clone)]
pub(super) struct PlannedPageWrite {
    pub(super) page_id: PageId,
    pub(super) image: Vec<u8>,
}

// A complete physical mutation to one file. The write path first builds this
// in memory, persists it as PL, and only then applies the page images.
#[derive(Clone, Default)]
pub(super) struct TimeSeriesFileMutationPlan {
    pub(super) create_file: bool,
    pub(super) delete_file: bool,
    pub(super) page_writes: Vec<PlannedPageWrite>,
    pub(super) next_page_count: Option<PageId>,
    pub(super) next_head_page_id: Option<Option<PageId>>,
    pub(super) next_tail_page_id: Option<Option<PageId>>,
}

impl TimeSeriesFile {
    pub(super) async fn persist_plan(&self, mut plan: TimeSeriesFileMutationPlan) -> RS<()> {
        scoped_task_trace!();
        // Physical WAL must reach durable storage before any data-page update.
        {
            let _stage = crate::server::stage_stats::StageGuard::new(
                crate::server::stage_stats::Stage::WrWalAppend,
            );
            if let Some(batch) = {
                let _diff_stage = crate::server::stage_stats::StageGuard::new(
                    crate::server::stage_stats::Stage::WrWalDiff,
                );
                self.build_pl_batch(&plan).await?
            } {
                trace!(
                    path = %self.path.display(),
                    create_file = plan.create_file,
                    delete_file = plan.delete_file,
                    page_writes = plan.page_writes.len(),
                    "time_series persist_plan append wal"
                );
                let backend = self.wal_backend.clone().ok_or_else(|| {
                    mudu_error!(ErrorCode::Internal, "missing time series wal backend")
                })?;
                // Serialize first to learn the batch's WAL LSN: every page
                // image published by this plan is stamped with it (before
                // checksum finalization) so recovery can tell which batches
                // a flushed page already reflects.
                let frames = {
                    let _encode_stage = crate::server::stage_stats::StageGuard::new(
                        crate::server::stage_stats::Stage::WrWalEncode,
                    );
                    let frames = backend.serialize_entry(&batch)?;
                    let start_lsn = frame_lsns(&frames)?.into_iter().next().ok_or_else(|| {
                        mudu_error!(ErrorCode::Internal, "wal entry serialized to no frames")
                    })?;
                    for write in &mut plan.page_writes {
                        stamp_page_image_lsn(&mut write.image, start_lsn)?;
                        finalize_page_image_checksum(&mut write.image)?;
                    }
                    frames
                };
                {
                    let _queue_stage = crate::server::stage_stats::StageGuard::new(
                        crate::server::stage_stats::Stage::WrWalQueue,
                    );
                    // Queue the PL frames into the same group-commit stream
                    // as the XL frames instead of writing them through
                    // inline: the flush round's merged write + fsync covers
                    // them, and the committing transaction's durability wait
                    // targets the last LSN it allocated (see kv.rs), so the
                    // PL batch is durable before the commit is reported.
                    let lsns = frame_lsns(&frames)?;
                    backend.enqueue_group_commit(frames, lsns, false).await?;
                }
            } else {
                // No WAL backend (standalone file): finalize the deferred
                // tailer checksums so the page-cache publish (and the later
                // dirty-page flush) carries valid checksums.
                for write in &mut plan.page_writes {
                    finalize_page_image_checksum(&mut write.image)?;
                }
            }
        }
        trace!(
            path = %self.path.display(),
            create_file = plan.create_file,
            delete_file = plan.delete_file,
            page_writes = plan.page_writes.len(),
            "time_series persist_plan apply plan"
        );
        let _stage = crate::server::stage_stats::StageGuard::new(
            crate::server::stage_stats::Stage::WrPublish,
        );
        self.apply_plan(&plan).await
    }

    pub(super) async fn apply_plan(&self, plan: &TimeSeriesFileMutationPlan) -> RS<()> {
        if plan.create_file {
            trace!(path = %self.path.display(), "time_series apply_plan create file start");
            match self.fs.as_ref() {
                Some(fs) => ensure_time_series_file_exists_async(fs.as_ref(), &self.path).await?,
                None => ensure_time_series_file_exists_async_no_fs(&self.path).await?,
            }
            trace!(path = %self.path.display(), "time_series apply_plan create file done");
        }
        // Publish section: from the first page-cache update to the last
        // metadata store there must be no await point. Read paths walk the
        // chain latch-free; an await here (this used to be the dirty-page
        // threshold flush) lets a reader observe a partially applied plan —
        // e.g. follow a next pointer to a page whose image and page_count
        // store have not happened yet (IndexOutOfRange).
        for write in &plan.page_writes {
            trace!(path = %self.path.display(), page_id = %write.page_id, "time_series apply_plan write page");
            self.apply_page_write(write.page_id, &write.image);
        }
        if plan.delete_file {
            close_file(self.take_file()?).await?;
            match self.fs.as_ref() {
                Some(fs) => fs.remove_file_if_exists(&self.path).await?,
                None => remove_file_if_exists_async(&self.path).await?,
            }
            self.page_cache.clear_sync();
            self.dirty_pages.clear_sync();
        }
        // Publication order matters for latch-free readers: page_count first,
        // so a reader that observes a new head/tail or a new chain link finds
        // the referenced page id already inside the valid range.
        if let Some(page_count) = plan.next_page_count {
            self.page_count
                .store(page_count.as_u64(), Ordering::Release);
        }
        if let Some(head_page_id) = plan.next_head_page_id {
            self.head_page_id
                .store(raw_page_id(head_page_id), Ordering::Release);
        }
        if let Some(tail_page_id) = plan.next_tail_page_id {
            self.tail_page_id
                .store(raw_page_id(tail_page_id), Ordering::Release);
        }
        // The dirty-page threshold flush is NOT triggered here: it ran inside
        // `write_latch` (and, above it, the relation write stripes),
        // serializing every writer of the file behind a full data-file
        // flush. Callers now run it after releasing the latch via
        // `flush_dirty_pages_if_over_threshold` — the publish section above
        // stays complete and await-free either way, so latch-free readers
        // still never observe a partially applied plan.
        Ok(())
    }

    pub(super) fn apply_page_write(&self, page_id: PageId, page: &[u8]) {
        // WAL-first deferred flush: the PL batch is already appended at
        // this point (see persist_plan), so the page image only updates the
        // page cache and marks the page dirty. The data file is written
        // later by flush_dirty_pages (background round, flush(), close(),
        // or the threshold flush in apply_plan), which coalesces
        // consecutive row writes to the same page into a single page write.
        // The image's tailer checksum was already finalized by persist_plan.
        //
        // Must stay synchronous: it runs inside apply_plan's publish
        // section.
        let _ = self.page_cache.remove_sync(&page_id);
        let _ = self
            .page_cache
            .insert_sync(page_id, Arc::new(page.to_vec()));
        let _ = self.dirty_pages.insert_sync(page_id);
    }

    /// Encodes the plan as record-level page deltas.
    ///
    /// Each planned page image is diffed against the page's published image
    /// (page cache or data file — the state the WAL replay will find as its
    /// base) and only the changed records and chain links are logged; a page
    /// allocated by this batch is logged as an init plus its record list.
    /// The published image is read before `apply_plan` publishes the new
    /// images, so it is exactly the previous batch state.
    pub(super) async fn build_pl_batch(
        &self,
        plan: &TimeSeriesFileMutationPlan,
    ) -> RS<Option<PLBatch>> {
        let Some(identity) = self.identity.as_ref() else {
            return Ok(None);
        };
        let mut ops = Vec::new();
        if plan.create_file {
            ops.push(PLOp::Create);
        }
        let published_page_count = self.page_count();
        for write in &plan.page_writes {
            if write.image.len() != PAGE_SIZE {
                return Err(mudu_error!(
                    ErrorCode::Encode,
                    format!(
                        "page write requires {} bytes, got {}",
                        PAGE_SIZE,
                        write.image.len()
                    )
                ));
            }
            let op = if write.page_id >= published_page_count {
                Some(Self::page_init_delta(write.page_id, &write.image)?)
            } else {
                let old_image = self.read_page(write.page_id).await?;
                Self::page_record_delta(write.page_id, &old_image, &write.image)?
            };
            if let Some(op) = op {
                ops.push(op);
            }
        }
        if plan.delete_file {
            ops.push(PLOp::Delete);
        }
        if ops.is_empty() {
            return Ok(None);
        }
        Ok(Some(PLBatch::new(vec![PLEntry {
            file: PLFileId {
                partition_id: identity.partition_id,
                table_id: identity.table_id,
                file_index: identity.file_index,
            },
            ops,
        }])))
    }

    /// Builds the delta for a page allocated by this batch: init metadata
    /// from the page header plus every record in slot order.
    fn page_init_delta(page_id: PageId, image: &[u8]) -> RS<PLOp> {
        let page = PageBlockRef::new(image);
        let header = page.header()?;
        let upserts = page_records(&page)?;
        Ok(PLOp::PageDelta(PageDelta {
            page_id,
            init: Some(PLPageInit {
                prev_page: header.prev_page(),
                next_page: header.next_page(),
                tuple_format_version: header.tuple_format_version(),
                tuple_schema_hash: header.tuple_schema_hash(),
                tuple_flags: header.tuple_flags(),
            }),
            links: None,
            removes: Vec::new(),
            upserts,
        }))
    }

    /// Diffs an existing page's published image against the batch's new
    /// image. Both slot arrays are sorted by `(timestamp, tuple_id)`, so the
    /// record diff is a single merge pass; identical payloads shared by both
    /// images are not logged. Returns `None` when nothing changed.
    fn page_record_delta(page_id: PageId, old_image: &[u8], new_image: &[u8]) -> RS<Option<PLOp>> {
        let old_page = PageBlockRef::new(old_image);
        let new_page = PageBlockRef::new(new_image);
        let old_header = old_page.header()?;
        let new_header = new_page.header()?;
        let links = if old_header.prev_page() != new_header.prev_page()
            || old_header.next_page() != new_header.next_page()
        {
            Some(PLPageLinks {
                prev_page: new_header.prev_page(),
                next_page: new_header.next_page(),
            })
        } else {
            None
        };

        let old_count = old_page.slot_count()?;
        let new_count = new_page.slot_count()?;
        let mut removes = Vec::new();
        let mut upserts = Vec::new();
        let mut old_idx = 0usize;
        let mut new_idx = 0usize;
        while old_idx < old_count || new_idx < new_count {
            if old_idx >= old_count {
                upserts.push(page_record(&new_page, new_idx)?);
                new_idx += 1;
                continue;
            }
            if new_idx >= new_count {
                removes.push(page_record_key(&old_page, old_idx)?);
                old_idx += 1;
                continue;
            }
            let old_slot = old_page.slot_ref(old_idx)?;
            let new_slot = new_page.slot_ref(new_idx)?;
            let old_key = (old_slot.timestamp(), old_slot.tuple_id());
            let new_key = (new_slot.timestamp(), new_slot.tuple_id());
            if old_key == new_key {
                if old_page.record_bytes(old_idx)? != new_page.record_bytes(new_idx)? {
                    upserts.push(page_record(&new_page, new_idx)?);
                }
                old_idx += 1;
                new_idx += 1;
            } else if old_key < new_key {
                removes.push(PLRecordKey {
                    timestamp: old_key.0,
                    tuple_id: old_key.1,
                });
                old_idx += 1;
            } else {
                upserts.push(page_record(&new_page, new_idx)?);
                new_idx += 1;
            }
        }

        if links.is_none() && removes.is_empty() && upserts.is_empty() {
            return Ok(None);
        }
        Ok(Some(PLOp::PageDelta(PageDelta {
            page_id,
            init: None,
            links,
            removes,
            upserts,
        })))
    }
}

/// Reads one record (key + payload) from a page in slot order.
fn page_record(page: &PageBlockRef<'_>, slot_index: usize) -> RS<PLRecord> {
    let slot = page.slot_ref(slot_index)?;
    Ok(PLRecord {
        timestamp: slot.timestamp(),
        tuple_id: slot.tuple_id(),
        payload: page.record_bytes(slot_index)?.to_vec(),
    })
}

/// Reads one record key from a page in slot order.
fn page_record_key(page: &PageBlockRef<'_>, slot_index: usize) -> RS<PLRecordKey> {
    let slot = page.slot_ref(slot_index)?;
    Ok(PLRecordKey {
        timestamp: slot.timestamp(),
        tuple_id: slot.tuple_id(),
    })
}

/// Reads every record of a page in slot order.
fn page_records(page: &PageBlockRef<'_>) -> RS<Vec<PLRecord>> {
    let count = page.slot_count()?;
    let mut records = Vec::with_capacity(count);
    for slot_index in 0..count {
        records.push(page_record(page, slot_index)?);
    }
    Ok(records)
}
