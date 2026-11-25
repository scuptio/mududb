//! Thread-confined core of a time-series file: a `page_buffer` clock cache
//! over the data file, the chain metadata, and the WAL backend. Lives on the
//! opener's worker thread; the public shell reaches it through a mailbox.

use super::io::remove_file_if_exists_async;
use super::{page_id_from_raw, raw_page_id, TimeSeriesFileIdobject};
use crate::page::page_block_ref::PAGE_SIZE;
use crate::page::PageId;
use ::wal::pl_batch::{new_pl_batch_writer, PLBatch};
use ::wal::pl_entry::{PLEntry, PLFileId, PLOp};
use ::wal::worker_log::ChunkedWorkerLogBackend;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_sys::contract::async_file::AsyncFile;
use mudu_sys::contract::async_fs::AsyncFs;
use mudu_sys::fs::SysFile;
use page_buffer::{
    AllocatorState, BufferManager, BufferOptions, ClockBufferManager, FilePageStore, LocalRwLock,
    PageImage, PageWriteback,
};
use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

/// Writeback safety net: every image reaching the data file passes through
/// tailer-checksum finalization, even if a future publish path forgets.
/// Images published by `persist_plan` are already finalized, so this is a
/// validity check plus a copy, never a rewrite.
#[derive(Debug)]
struct TsPageWriteback;
static TS_PAGE_WRITEBACK: TsPageWriteback = TsPageWriteback;
impl PageWriteback for TsPageWriteback {
    fn encode(&self, bytes: &[u8]) -> RS<Vec<u8>> {
        if super::page::page_image_checksum_valid(bytes)? {
            return Ok(bytes.to_vec());
        }
        let mut image = bytes.to_vec();
        super::page::finalize_page_image_checksum(&mut image)?;
        Ok(image)
    }
}

/// Builds a resident image carrying the time-series writeback policy.
pub(super) fn resident_image(bytes: Vec<u8>) -> PageImage {
    PageImage::with_writeback(bytes, &TS_PAGE_WRITEBACK)
}

/// Maps a file page number to the buffer's logical page number. The buffer
/// reserves logical page zero; the data file numbers pages from zero.
pub(super) fn buffer_page_id(page_id: PageId) -> u64 {
    page_id.as_u64() + 1
}

/// Thread-confined core of a [`super::TimeSeriesFile`].
///
/// The buffer bounds page residency; a dirty frame is written back on
/// eviction, on a threshold/background round, or on `flush`/`close`. Every
/// dirty image is WAL-covered by the time it is published, so writeback
/// ordering carries no durability contract.
pub(in crate::time_series) struct TsFileInner {
    // Relation-owned files carry a stable identity and a dedicated PL backend.
    // Standalone test files leave both fields as `None`.
    pub(super) fs: Option<Arc<dyn AsyncFs>>,
    pub(super) identity: Option<TimeSeriesFileIdobject>,
    pub(super) path: PathBuf,
    pub(super) file: SysFile,
    pub(super) wal_backend: Option<ChunkedWorkerLogBackend>,
    pub(super) buffer: Rc<ClockBufferManager>,
    // Serializes the whole cross-await write path of one file (locate insert
    // point -> build plan -> WAL persist -> apply). Read paths never take
    // this latch: writers mutate the chain append-only and readers may
    // observe the old or the new head, with MVCC visibility arbitrating
    // above.
    pub(super) write_latch: LocalRwLock<()>,
    // Chain metadata lives in plain cells: every access runs on the owning
    // thread, and publication batches the stores between await points.
    pub(super) page_count: Cell<u64>,
    // `NONE_PAGE_ID` (u64::MAX) encodes `None`; valid page ids never reach it.
    pub(super) head_page_id: Cell<u64>,
    pub(super) tail_page_id: Cell<u64>,
    pub(super) tuple_format_version: u32,
    pub(super) tuple_schema_hash: u64,
    pub(super) tuple_flags: u64,
    // A failure while applying an already-logged batch leaves the in-memory
    // state skewed from the WAL stream; the file refuses further mutations
    // and must be reopened (replay converges the data file).
    poisoned: RefCell<Option<String>>,
    closed: Cell<bool>,
}

impl TsFileInner {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        fs: Option<Arc<dyn AsyncFs>>,
        path: PathBuf,
        identity: Option<TimeSeriesFileIdobject>,
        wal_backend: Option<ChunkedWorkerLogBackend>,
        file: Arc<dyn AsyncFile>,
        page_count: PageId,
        head_page_id: Option<PageId>,
        tail_page_id: Option<PageId>,
        tuple_schema_hash: u64,
    ) -> RS<Self> {
        let options = BufferOptions {
            page_size: PAGE_SIZE,
            frames: super::TS_BUFFER_FRAMES,
            partitions: 4,
            swizzling: true,
            prefetch: false,
        };
        let store = FilePageStore::with_page_bias(file.clone(), PAGE_SIZE, 1)?;
        let buffer = ClockBufferManager::new(
            options,
            Arc::new(store),
            AllocatorState {
                next_page: page_count.as_u64() + 1,
            },
            None,
        )?;
        Ok(Self {
            fs,
            identity,
            path,
            file: SysFile::new(file),
            wal_backend,
            buffer: Rc::new(buffer),
            write_latch: LocalRwLock::new(()),
            page_count: Cell::new(page_count.as_u64()),
            head_page_id: Cell::new(raw_page_id(head_page_id)),
            tail_page_id: Cell::new(raw_page_id(tail_page_id)),
            tuple_format_version: if tuple_schema_hash != 0 { 1 } else { 0 },
            tuple_schema_hash,
            tuple_flags: 0,
            poisoned: RefCell::new(None),
            closed: Cell::new(false),
        })
    }

    pub(super) fn page_count(&self) -> PageId {
        PageId::new(self.page_count.get())
    }

    pub(super) fn head_page_id(&self) -> Option<PageId> {
        page_id_from_raw(self.head_page_id.get())
    }

    pub(super) fn tail_page_id(&self) -> Option<PageId> {
        page_id_from_raw(self.tail_page_id.get())
    }

    /// Number of pages currently dirty (published but not yet written back
    /// to the data file).
    pub(super) fn dirty_page_count(&self) -> usize {
        self.buffer.dirty_count()
    }

    /// Idobject and recovered WAL extent used to reject stale rebuildable indexes.
    pub(super) fn index_cache_stamp(&self) -> Vec<u64> {
        let (table, partition) = self
            .identity
            .as_ref()
            .map(|i| (i.table_id, i.partition_id))
            .unwrap_or((0, 0));
        vec![
            table as u64,
            (table >> 64) as u64,
            partition as u64,
            (partition >> 64) as u64,
            self.tuple_schema_hash,
            self.tuple_format_version as u64,
            self.tuple_flags,
            self.wal_backend
                .as_ref()
                .map(|w| w.next_lsn().as_u64())
                .unwrap_or(0),
            self.page_count.get(),
            self.head_page_id.get(),
            self.tail_page_id.get(),
        ]
    }

    /// Returns the newest published image of `page_id`.
    ///
    /// A resident page is read without suspending, keeping cache hits
    /// await-free for chain walks. A miss may evict another page, flushing
    /// it first when it is dirty (always WAL-covered by publication time).
    pub(super) async fn read_page(&self, page_id: PageId) -> RS<PageImage> {
        mudu_utils::scoped_task_trace!();
        let page_count = self.page_count();
        if page_id >= page_count {
            return Err(mudu_error!(
                ErrorCode::IndexOutOfRange,
                format!("page {} out of range {}", page_id, page_count)
            ));
        }
        let page = self.buffer.page_ref(buffer_page_id(page_id));
        if let Some(access) = self.buffer.try_access() {
            if let Some(pin) = self.buffer.try_pin(&access, &page)? {
                return Ok(pin.snapshot().await);
            }
        }
        let access = self.buffer.access().await;
        let pin = self.buffer.pin(&access, &page).await?;
        Ok(pin.snapshot().await)
    }

    /// Rejects mutations on a poisoned or closed file.
    pub(super) fn check_writable(&self) -> RS<()> {
        if let Some(cause) = self.poisoned.borrow().as_ref() {
            return Err(mudu_error!(
                ErrorCode::Storage,
                format!("time series file poisoned, reopen to recover: {cause}")
            ));
        }
        if self.closed.get() {
            return Err(mudu_error!(
                ErrorCode::Internal,
                "time series file is closed"
            ));
        }
        Ok(())
    }

    /// Rejects reads on a closed file. A poisoned file still serves reads:
    /// every published page image is self-consistent and every page id a new
    /// image references was admitted first, so chain walks stay well-formed.
    pub(super) fn check_open(&self) -> RS<()> {
        if self.closed.get() {
            return Err(mudu_error!(
                ErrorCode::Internal,
                "time series file is closed"
            ));
        }
        Ok(())
    }

    /// Structs why the file was poisoned; the first cause sticks.
    pub(super) fn poison(&self, cause: String) {
        let mut slot = self.poisoned.borrow_mut();
        if slot.is_none() {
            *slot = Some(cause);
        }
    }

    /// Flushes dirty data pages and then fsyncs the file.
    pub(super) async fn flush(&self) -> RS<()> {
        let access = self.buffer.access().await;
        self.buffer.flush(&access).await
    }

    /// Writes back dirty data pages without forcing durability. Durability
    /// comes from the PL WAL; the data file only needs to catch up before
    /// the WAL chunks could be reclaimed.
    pub(super) async fn flush_dirty_pages(&self) -> RS<()> {
        let access = self.buffer.access().await;
        self.buffer.writeback_dirty(&access).await
    }

    /// Flushes dirty pages when the writeback watermark is reached. Runs
    /// OUTSIDE `write_latch`: the flush is a memory backstop (durability
    /// comes from the WAL), and the buffer's dirty epochs already make it
    /// safe against concurrent publishers.
    pub(super) async fn flush_dirty_pages_if_over_threshold(&self) -> RS<()> {
        if self.buffer.dirty_count() >= super::DIRTY_PAGE_FLUSH_THRESHOLD {
            self.flush_dirty_pages().await?;
        }
        Ok(())
    }

    /// Drives the WAL backend's group-commit queue to durable storage.
    pub(super) async fn flush_wal(&self) -> RS<()> {
        if let Some(backend) = &self.wal_backend {
            backend.force_flush_log_async().await?;
            backend.fsync_unsynced_paths().await?;
        }
        Ok(())
    }

    /// Flushes the WAL group-commit queue and the dirty data pages, then
    /// closes the file. Writes are durable once this returns.
    pub(super) async fn close(&self) -> RS<()> {
        self.closed.set(true);
        self.flush_wal().await?;
        self.flush().await?;
        self.file.close().await
    }

    /// Appends a delete record to the file's WAL (for a relation-owned
    /// file), then closes and removes the data file. Dirty frames are
    /// discarded with the buffer: the WAL delete record redoes the removal.
    pub(super) async fn delete_file(&self) -> RS<()> {
        if let Some(identity) = self.identity.as_ref() {
            let backend = self.wal_backend.clone().ok_or_else(|| {
                mudu_error!(ErrorCode::Internal, "missing time series wal backend")
            })?;
            new_pl_batch_writer(backend)
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
        self.closed.set(true);
        self.file.close().await?;
        match self.fs.as_ref() {
            Some(fs) => fs.remove_file_if_exists(&self.path).await,
            None => remove_file_if_exists_async(&self.path).await,
        }
    }
}

/// Poisons the file when the guarded section exits without success: a batch
/// that was already appended to the WAL must not leave a partially applied
/// in-memory state for later batches to build on.
pub(super) struct PoisonGuard<'a> {
    inner: &'a TsFileInner,
    armed: bool,
}
impl<'a> PoisonGuard<'a> {
    pub(super) fn new(inner: &'a TsFileInner) -> Self {
        Self { inner, armed: true }
    }
    pub(super) fn disarm(mut self) {
        self.armed = false;
    }
}
impl Drop for PoisonGuard<'_> {
    fn drop(&mut self) {
        if self.armed {
            self.inner
                .poison("publish interrupted after WAL append".to_string());
        }
    }
}
