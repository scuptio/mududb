use crate::storage::page::page_header::NONE_PAGE_ID;
use crate::storage::page::PageId;
use crate::wal::pl_batch::{new_pl_batch_writer, PLBatch};
use crate::wal::pl_entry::{PLEntry, PLFileId, PLOp};
use crate::wal::worker_log::ChunkedWorkerLogBackend;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_sys::contract::async_fs::AsyncFs;
use mudu_sys::fs::SysFile;
use mudu_sys::sync::async_::AMutex;
use mudu_sys::sync::SMutex;
use scc::{HashMap, HashSet};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

const FILE_MODE_644: u32 = 0o644;
const RELATION_WAL_CHUNK_SIZE: u64 = 256 * 1024;

/// Upper bound on pages written to the page cache but not yet flushed to
/// the data file. A writer that crosses it synchronously flushes a batch
/// while holding the write latch, so deferred page writes (WAL-first dirty
/// pages) stay memory-bounded. This is a performance watermark, not a
/// correctness mechanism: every dirty page is already covered by the PL
/// WAL, which currently has no GC (WAL chunk reclamation is future work),
/// so an unflushed dirty page is always replayed on open.
const DIRTY_PAGE_FLUSH_THRESHOLD: usize = 256;

/// Logical identity for one physical time-series file.
///
/// The relation layer assigns `file_index` values and WAL only works with this
/// numeric identity, never with `"key"` / `"value"` strings.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TimeSeriesFileIdentity {
    pub partition_id: OID,
    pub table_id: OID,
    pub file_index: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TimeSeriesRecord {
    pub timestamp: u64,
    pub tuple_id: u64,
    pub payload: Vec<u8>,
    pub page_id: PageId,
    pub slot_index: usize,
}

pub struct TimeSeriesFile {
    // Relation-owned files carry a stable identity and a dedicated PL backend.
    // Standalone test files leave both fields as `None`.
    fs: Option<Arc<dyn AsyncFs>>,
    identity: Option<TimeSeriesFileIdentity>,
    path: PathBuf,
    // The open file handle. SysFile is an Arc clone inside, so readers clone
    // it out of the SMutex instead of holding the mutex across I/O.
    file: SMutex<Option<SysFile>>,
    wal_backend: Option<ChunkedWorkerLogBackend>,
    // Cached page images shared by reference: cache hits clone the Arc
    // instead of memcpy-ing 4 KiB per read.
    page_cache: HashMap<PageId, Arc<Vec<u8>>>,
    // Pages whose newest image lives only in `page_cache` and has not been
    // written to the data file yet. WAL-first deferred flush: persist_plan
    // appends the PL batch before the image is cached, so recovery never
    // depends on a dirty page being flushed to the data file.
    dirty_pages: HashSet<PageId>,
    // Serializes flush_dirty_pages rounds. Writers never write to the data
    // file themselves, so this latch is the only thing keeping an older
    // image from being written over a newer one when two flush rounds
    // overlap.
    dirty_flush_latch: AMutex<()>,
    // Serializes the whole cross-await write path of one file (locate insert
    // point -> build plan -> WAL persist -> apply). The chain read-modify-
    // write spans awaited page reads and page writes, so a sync lock cannot
    // be used here. Read paths (get/scan_range/read_page) never take this
    // latch: writers mutate the chain append-only and readers may observe
    // the old or the new head, with MVCC visibility arbitrating above.
    write_latch: AMutex<()>,
    // Chain metadata lives in atomics so latch-free readers can observe it.
    // Writers only mutate these while holding `write_latch`.
    page_count: AtomicU64,
    // `NONE_PAGE_ID` (u64::MAX) encodes `None`; valid page ids never reach it.
    head_page_id: AtomicU64,
    tail_page_id: AtomicU64,
    tuple_format_version: u32,
    tuple_schema_hash: u64,
    tuple_flags: u64,
}

fn raw_page_id(page_id: Option<PageId>) -> u64 {
    page_id
        .map(|id| id.as_u64())
        .unwrap_or(NONE_PAGE_ID.as_u64())
}

fn page_id_from_raw(raw: u64) -> Option<PageId> {
    if raw == NONE_PAGE_ID.as_u64() {
        None
    } else {
        Some(PageId::new(raw))
    }
}

/// Returns an image whose tailer checksum is valid for persistence. Images
/// published by `persist_plan` are already finalized, so this is normally a
/// cheap verification; an image that somehow reached the cache with a stale
/// checksum is copied and fixed before it touches the data file, leaving the
/// shared cached image untouched.
fn persisted_page_image(image: &Arc<Vec<u8>>) -> RS<Arc<Vec<u8>>> {
    if page::page_image_checksum_valid(image)? {
        return Ok(image.clone());
    }
    let mut fixed = image.as_ref().clone();
    page::finalize_page_image_checksum(&mut fixed)?;
    Ok(Arc::new(fixed))
}

/// One dirty page queued for a batched flush: the page id, the cached image
/// (kept for the `Arc::ptr_eq` dirty-mark check), and the image actually
/// written to disk (checksum-finalized).
type DirtyPageWrite = (PageId, Arc<Vec<u8>>, Arc<Vec<u8>>);

impl TimeSeriesFile {
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn identity(&self) -> Option<&TimeSeriesFileIdentity> {
        self.identity.as_ref()
    }

    pub fn page_count(&self) -> PageId {
        PageId::new(self.page_count.load(Ordering::Acquire))
    }

    pub fn head_page_id(&self) -> Option<PageId> {
        page_id_from_raw(self.head_page_id.load(Ordering::Acquire))
    }

    pub fn tail_page_id(&self) -> Option<PageId> {
        page_id_from_raw(self.tail_page_id.load(Ordering::Acquire))
    }

    fn file_ref(&self) -> RS<SysFile> {
        self.file
            .lock()?
            .as_ref()
            .cloned()
            .ok_or_else(|| mudu_error!(ErrorCode::Internal, "time series file is not open"))
    }

    fn take_file(&self) -> RS<SysFile> {
        self.file
            .lock()?
            .take()
            .ok_or_else(|| mudu_error!(ErrorCode::Internal, "time series file is not open"))
    }

    /// Flushes dirty data pages and then fsyncs the file. Before the
    /// deferred-flush change this only fsynced; tests that rely on `flush`
    /// making writes visible to a fresh open keep that semantic.
    pub async fn flush(&self) -> RS<()> {
        self.flush_dirty_pages().await?;
        io::flush_file(&self.file_ref()?).await
    }

    pub async fn close(self) -> RS<()> {
        // PL frames are queued, not written through: drain the group-commit
        // queue before closing so close keeps its "writes are durable"
        // semantic (production workers drive the same flush continuously).
        self.flush_wal_async().await?;
        self.flush_dirty_pages().await?;
        io::close_file(self.take_file()?).await
    }

    pub fn close_sync(self) -> RS<()> {
        self.flush_dirty_pages_sync()?;
        drop(self.take_file()?);
        Ok(())
    }

    /// Number of pages currently marked dirty (cached but not yet written
    /// to the data file).
    pub fn dirty_page_count(&self) -> usize {
        self.dirty_pages.len()
    }

    /// Drives the WAL backend's group-commit queue to durable storage.
    /// PL frames are queued (not written through) by `persist_plan`, so
    /// closing a file calls this to keep the "close makes writes durable"
    /// semantic; production workers drive the same flush from the commit
    /// path and the worker event loop. In periodic sync mode the flush
    /// round may skip the fsync, so the dirty chunks are fsynced explicitly
    /// afterwards.
    pub(crate) async fn flush_wal_async(&self) -> RS<()> {
        if let Some(backend) = &self.wal_backend {
            backend.force_flush_log_async().await?;
            backend.fsync_unsynced_paths().await?;
        }
        Ok(())
    }

    /// Writes every dirty page image from `page_cache` to the data file and
    /// clears its dirty mark. A page whose write fails keeps its mark so
    /// the next round retries it; a page re-dirtied while its image was in
    /// flight keeps the mark as well (the cached image is re-checked with
    /// `Arc::ptr_eq` before clearing).
    ///
    /// Pages are written in batches of [`DIRTY_FLUSH_BATCH`] concurrent
    /// writes instead of one awaited write per page: on io_uring workers the
    /// submissions all go out before the completions are awaited (QD>1), on
    /// tokio the per-file mutex serializes them as before. This matters most
    /// for the threshold flush, which now runs outside `write_latch` (see
    /// `flush_dirty_pages_if_over_threshold`) but must still not stall the
    /// loop for one round trip per page.
    ///
    /// Every [`DIRTY_FLUSH_YIELD_PAGES`] processed pages the sweep yields
    /// cooperatively, so a large dirty set cannot monopolize the worker
    /// event loop this flush runs on.
    pub async fn flush_dirty_pages(&self) -> RS<()> {
        const DIRTY_FLUSH_BATCH: usize = 16;
        /// Processed dirty pages between two cooperative yields.
        const DIRTY_FLUSH_YIELD_PAGES: usize = 32;
        if self.dirty_pages.is_empty() {
            return Ok(());
        }
        let _guard = self.dirty_flush_latch.lock().await;
        let file = self.file_ref()?;
        let mut batch: Vec<DirtyPageWrite> = Vec::with_capacity(DIRTY_FLUSH_BATCH);
        let mut first_err = None;
        let mut since_yield = 0usize;
        for (page_id, image) in self.dirty_page_images() {
            let persisted = match persisted_page_image(&image) {
                Ok(persisted) => persisted,
                Err(err) => {
                    if first_err.is_none() {
                        first_err = Some(err);
                    }
                    continue;
                }
            };
            batch.push((page_id, image, persisted));
            if batch.len() >= DIRTY_FLUSH_BATCH {
                if let Err(err) = self.flush_dirty_batch(&file, &mut batch).await {
                    if first_err.is_none() {
                        first_err = Some(err);
                    }
                }
                since_yield += DIRTY_FLUSH_BATCH;
                if since_yield >= DIRTY_FLUSH_YIELD_PAGES {
                    since_yield = 0;
                    crate::common::yield_now::cooperative_yield_now().await;
                }
            }
        }
        if !batch.is_empty() {
            if let Err(err) = self.flush_dirty_batch(&file, &mut batch).await {
                if first_err.is_none() {
                    first_err = Some(err);
                }
            }
        }
        match first_err {
            Some(err) => Err(err),
            None => Ok(()),
        }
    }

    /// Writes one batch of dirty pages concurrently, then clears the dirty
    /// marks of the pages that were persisted successfully. Failed pages
    /// keep their marks; the first error is returned.
    async fn flush_dirty_batch(&self, file: &SysFile, batch: &mut Vec<DirtyPageWrite>) -> RS<()> {
        let mut writes = Vec::with_capacity(batch.len());
        for (page_id, _, persisted) in batch.iter() {
            let offset = io::page_offset(*page_id)?;
            writes.push(async move { file.write_all_at(offset, persisted.as_slice()).await });
        }
        let results = futures::future::join_all(writes).await;
        let mut first_err = None;
        for ((page_id, image, _), result) in batch.drain(..).zip(results) {
            match result {
                Ok(()) => self.clear_dirty_mark(page_id, &image),
                Err(err) => {
                    if first_err.is_none() {
                        first_err = Some(err);
                    }
                }
            }
        }
        match first_err {
            Some(err) => Err(err),
            None => Ok(()),
        }
    }

    /// Synchronous dirty-page flush for `close_sync`, which cannot await.
    /// No `dirty_flush_latch` is taken: `close_sync` consumes `self`, so no
    /// concurrent flush round can be in flight.
    fn flush_dirty_pages_sync(&self) -> RS<()> {
        if self.dirty_pages.is_empty() {
            return Ok(());
        }
        let file = self.file_ref()?;
        for (page_id, image) in self.dirty_page_images() {
            let persisted = persisted_page_image(&image)?;
            io::sync_write_all_at(&file, io::page_offset(page_id)?, &persisted)?;
            self.clear_dirty_mark(page_id, &image);
        }
        Ok(())
    }

    /// Snapshots the dirty page images currently in the cache, ordered by
    /// page id. Dirty marks whose image is no longer cached (the cache has
    /// no eviction today, so this only happens after a delete cleared the
    /// cache) are dropped: there is nothing left to write for them.
    fn dirty_page_images(&self) -> Vec<(PageId, Arc<Vec<u8>>)> {
        let mut stale = Vec::new();
        let mut images = Vec::with_capacity(self.dirty_pages.len());
        self.dirty_pages.iter_sync(|page_id| {
            match self.page_cache.read_sync(page_id, |_, image| image.clone()) {
                Some(image) => images.push((*page_id, image)),
                None => stale.push(*page_id),
            }
            true
        });
        for page_id in stale {
            let _ = self.dirty_pages.remove_sync(&page_id);
        }
        images.sort_by_key(|(page_id, _)| *page_id);
        images
    }

    fn clear_dirty_mark(&self, page_id: PageId, flushed: &Arc<Vec<u8>>) {
        // Only clear the mark when the cached image is still the one that
        // was written; a concurrent apply may have installed a newer image,
        // in which case the newer dirty mark must survive.
        let current = self
            .page_cache
            .read_sync(&page_id, |_, image| Arc::ptr_eq(image, flushed));
        if current == Some(true) {
            let _ = self.dirty_pages.remove_sync(&page_id);
        }
    }

    pub async fn delete_file(self) -> RS<()> {
        if let Some(identity) = self.identity.as_ref() {
            let backend = self.wal_backend.clone().ok_or_else(|| {
                mudu_error!(ErrorCode::Internal, "missing time series wal backend")
            })?;
            let writer = new_pl_batch_writer(backend);
            writer
                .append(&PLBatch::new(vec![PLEntry {
                    file: PLFileId {
                        partition_id: identity.partition_id,
                        table_id: identity.table_id,
                        file_index: identity.file_index,
                    },
                    ops: vec![PLOp::Delete],
                }]))
                .await?;
        }
        io::close_file(self.take_file()?).await?;
        match self.fs.as_ref() {
            Some(fs) => fs.remove_file_if_exists(&self.path).await,
            None => io::remove_file_if_exists_async(&self.path).await,
        }
    }
}

mod io;
mod open;
mod page;
mod plan;
mod read;
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests;
mod wal;
mod write;
