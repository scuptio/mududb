use super::inner::{buffer_page_id, PoisonGuard, TsFileInner};
use super::io::{ensure_time_series_file_exists_async, ensure_time_series_file_exists_async_no_fs};
use super::page::{finalize_page_image_checksum, stamp_page_image_lsn};
use crate::page::page_block_ref::{PageBlockRef, PAGE_SIZE};
use crate::page::PageId;
use ::wal::log_frame::frame_lsns;
use ::wal::pl_batch::PLBatch;
use ::wal::pl_entry::{
    PLEntry, PLFileId, PLOp, PLPageInit, PLPageLinks, PLStruct, PLStructKey, PageDelta,
};
use ::wal::worker_log::WorkerLogBackend;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_utils::scoped_task_trace;
use page_buffer::BufferManager;
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

impl TsFileInner {
    pub(super) async fn persist_plan(&self, mut plan: TimeSeriesFileMutationPlan) -> RS<()> {
        scoped_task_trace!();
        // Physical WAL must reach durable storage before any data-page update.
        {
            let _stage =
                ::catalog::stage_stats::StageGuard::new(::catalog::stage_stats::Stage::WrWalAppend);
            if let Some(batch) = {
                let _diff_stage = ::catalog::stage_stats::StageGuard::new(
                    ::catalog::stage_stats::Stage::WrWalDiff,
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
                    let _encode_stage = ::catalog::stage_stats::StageGuard::new(
                        ::catalog::stage_stats::Stage::WrWalEncode,
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
                    let _queue_stage = ::catalog::stage_stats::StageGuard::new(
                        ::catalog::stage_stats::Stage::WrWalQueue,
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
                // tailer checksums so the buffer publish (and the later
                // writeback) carries valid checksums.
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
        let _stage =
            ::catalog::stage_stats::StageGuard::new(::catalog::stage_stats::Stage::WrPublish);
        self.apply_plan(&plan).await
    }

    /// Publishes the plan's page images into the buffer and stores the chain
    /// metadata. Admission of batch-allocated pages completes before any
    /// existing page's new image can be observed, so a chain reader can
    /// never be referred to a page that is not yet readable. A failure (or
    /// cancellation) midway leaves the in-memory state skewed from the
    /// already-appended WAL batch; the file is poisoned and must be
    /// reopened, with replay converging the data file.
    pub(super) async fn apply_plan(&self, plan: &TimeSeriesFileMutationPlan) -> RS<()> {
        if plan.create_file {
            trace!(path = %self.path.display(), "time_series apply_plan create file start");
            match self.fs.as_ref() {
                Some(fs) => ensure_time_series_file_exists_async(fs.as_ref(), &self.path).await?,
                None => ensure_time_series_file_exists_async_no_fs(&self.path).await?,
            }
            trace!(path = %self.path.display(), "time_series apply_plan create file done");
        }
        let guard = PoisonGuard::new(self);
        let published_page_count = self.page_count();
        // Admission phase: batch-allocated pages become resident-dirty. The
        // buffer's allocator state stays in lockstep with the file's page
        // count because admission happens exactly once per planned new page.
        for write in &plan.page_writes {
            if write.page_id >= published_page_count {
                trace!(path = %self.path.display(), page_id = %write.page_id, "time_series apply_plan admit page");
                self.admit_new_page(write).await?;
            }
        }
        // Publish phase: replace existing pages' images. Awaiting between
        // pages is safe: every page id a new image references was admitted
        // above, and the chain metadata below is stored only after all
        // images are in place (all synchronous, no yield in between).
        for write in &plan.page_writes {
            if write.page_id < published_page_count {
                trace!(path = %self.path.display(), page_id = %write.page_id, "time_series apply_plan write page");
                self.publish_page_write(write).await?;
            }
        }
        if let Some(page_count) = plan.next_page_count {
            self.page_count.set(page_count.as_u64());
        }
        if let Some(head_page_id) = plan.next_head_page_id {
            self.head_page_id.set(super::raw_page_id(head_page_id));
        }
        if let Some(tail_page_id) = plan.next_tail_page_id {
            self.tail_page_id.set(super::raw_page_id(tail_page_id));
        }
        if plan.delete_file {
            self.file.close().await?;
            match self.fs.as_ref() {
                Some(fs) => fs.remove_file_if_exists(&self.path).await?,
                None => super::io::remove_file_if_exists_async(&self.path).await?,
            }
        }
        guard.disarm();
        // The dirty-page threshold flush is NOT triggered here: it would run
        // inside `write_latch`, serializing every writer of this file behind
        // a full data-file writeback. Callers run it after releasing the
        // latch via `flush_dirty_pages_if_over_threshold`.
        Ok(())
    }

    /// Publishes one batch-allocated page as a resident-dirty frame without
    /// a backing write; the WAL batch already covers its durability.
    async fn admit_new_page(&self, write: &PlannedPageWrite) -> RS<()> {
        let access = self.buffer.access().await;
        let reservation = self.buffer.reserve_page(&access).await?;
        if reservation.id() != buffer_page_id(write.page_id) {
            return Err(mudu_error!(
                ErrorCode::Internal,
                "buffer page allocation diverged from file plan"
            ));
        }
        self.buffer
            .initialize_resident(
                &access,
                reservation,
                super::inner::resident_image(write.image.clone()),
            )
            .await?;
        Ok(())
    }

    /// Replaces the buffer's image of an existing page with the plan's
    /// image, marking the frame dirty.
    async fn publish_page_write(&self, write: &PlannedPageWrite) -> RS<()> {
        let access = self.buffer.access().await;
        let page = self.buffer.page_ref(buffer_page_id(write.page_id));
        let pin = self.buffer.pin(&access, &page).await?;
        let mut prepared = pin
            .write()
            .await
            .prepare(super::inner::resident_image(write.image.clone()))?;
        prepared.publish();
        Ok(())
    }

    /// Encodes the plan as record-level page deltas.
    ///
    /// Each planned page image is diffed against the page's published image
    /// (resident in the buffer or read from the data file — the state the
    /// WAL replay will find as its base) and only the changed records and
    /// chain links are logged; a page allocated by this batch is logged as
    /// an init plus its record list. The published image is read before
    /// `apply_plan` publishes the new images, so it is exactly the previous
    /// batch state.
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
            let (old_index, new_index) = if old_idx >= old_count {
                (None, Some(new_idx))
            } else if new_idx >= new_count {
                (Some(old_idx), None)
            } else {
                let old_slot = old_page.slot_ref(old_idx)?;
                let new_slot = new_page.slot_ref(new_idx)?;
                let old_key = (old_slot.timestamp(), old_slot.tuple_id());
                let new_key = (new_slot.timestamp(), new_slot.tuple_id());
                if old_key == new_key {
                    (Some(old_idx), Some(new_idx))
                } else if crate::page::sequence_edit::key_less(
                    old_key.0, old_key.1, new_key.0, new_key.1,
                ) {
                    (Some(old_idx), None)
                } else {
                    (None, Some(new_idx))
                }
            };
            let equal = match (old_index, new_index) {
                (Some(old), Some(new)) => {
                    old_page.record_bytes(old)? == new_page.record_bytes(new)?
                }
                _ => false,
            };
            match super::delta_value::diff(old_index.is_some(), new_index, equal) {
                super::delta_value::Change::Keep => {}
                super::delta_value::Change::Set(index) => {
                    upserts.push(page_record(&new_page, index)?)
                }
                super::delta_value::Change::Remove => {
                    removes.push(page_record_key(&old_page, old_idx)?)
                }
            }
            if old_index.is_some() {
                old_idx += 1;
            }
            if new_index.is_some() {
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
fn page_record(page: &PageBlockRef<'_>, slot_index: usize) -> RS<PLStruct> {
    let slot = page.slot_ref(slot_index)?;
    Ok(PLStruct {
        timestamp: slot.timestamp(),
        tuple_id: slot.tuple_id(),
        payload: page.record_bytes(slot_index)?.to_vec(),
    })
}

/// Reads one record key from a page in slot order.
fn page_record_key(page: &PageBlockRef<'_>, slot_index: usize) -> RS<PLStructKey> {
    let slot = page.slot_ref(slot_index)?;
    Ok(PLStructKey {
        timestamp: slot.timestamp(),
        tuple_id: slot.tuple_id(),
    })
}

/// Reads every record of a page in slot order.
fn page_records(page: &PageBlockRef<'_>) -> RS<Vec<PLStruct>> {
    let count = page.slot_count()?;
    let mut records = Vec::with_capacity(count);
    for slot_index in 0..count {
        records.push(page_record(page, slot_index)?);
    }
    Ok(records)
}

#[cfg(test)]
mod delta_tests {
    use super::*;

    #[test]
    fn generated_record_delta_reconstructs_new_logical_page() -> RS<()> {
        let mut old = vec![0u8; PAGE_SIZE];
        let mut page = crate::page::page_block_ref_mut::PageBlockRefMut::new(&mut old);
        page.init_empty(1u64.into())?;
        page.insert_record(1, 1, b"old")?;
        page.insert_record(2, 2, b"remove")?;
        page.insert_record(3, 3, b"unchanged")?;
        let mut new = old.clone();
        let mut page = crate::page::page_block_ref_mut::PageBlockRefMut::new(&mut new);
        page.delete_record(1)?;
        page.update_record(0, 1, 1, b"new-longer-payload")?;
        page.insert_record(4, 4, b"insert")?;
        page.set_page_links(7u64.into(), 9u64.into())?;
        let op = TsFileInner::page_record_delta(1u64.into(), &old, &new)?;
        let Some(PLOp::PageDelta(delta)) = op else {
            return Err(mudu_error!(ErrorCode::Internal, "expected nonempty delta"));
        };
        assert_eq!(delta.removes.len(), 1);
        assert_eq!(delta.upserts.len(), 2);
        let mut replay = super::super::wal::recovered_base_image(&delta, Some(old))?;
        super::super::wal::apply_recovered_upserts(&mut replay, &delta)?;
        assert_eq!(
            page_records(&PageBlockRef::new(&replay))?,
            page_records(&PageBlockRef::new(&new))?
        );
        let actual = PageBlockRef::new(&replay).header()?;
        assert_eq!(actual.prev_page(), 7u64);
        assert_eq!(actual.next_page(), 9u64);
        assert!(TsFileInner::page_record_delta(1u64.into(), &replay, &new)?.is_none());
        Ok(())
    }
}
