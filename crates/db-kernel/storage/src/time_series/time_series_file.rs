//! Append-optimized record file built from a chain of slotted pages, with a
//! dedicated physical WAL stream for relation-owned files.
//!
//! Page residency, eviction and writeback are delegated to the shared
//! `page_buffer` clock cache, so a file's memory footprint stays bounded;
//! the WAL-first durability contract is kept by this layer above it. The
//! cache is thread-confined and lives on the opener's worker thread; this
//! [`TimeSeriesFile`] is the sendable mailbox shell that reaches it (the
//! same pattern the relation paged index uses).

use crate::mailbox::Mailbox;
use crate::page::PageId;
use ::wal::worker_log::ChunkedWorkerLogBackend;
use inner::TsFileInner;
use mudu::common::id::OID;
use mudu::common::result::RS;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const FILE_MODE_644: u32 = 0o644;
const RELATION_WAL_CHUNK_SIZE: u64 = 256 * 1024;

/// Upper bound on pages published dirty but not yet written back to the data
/// file. A writer that crosses it triggers a writeback round after releasing
/// the write latch, so deferred page writes (WAL-first dirty pages) stay
/// memory-bounded. This is a performance watermark, not a correctness
/// mechanism: every dirty page is already covered by the PL WAL, which
/// currently has no GC (WAL chunk reclamation is future work), so an
/// unwritten dirty page is always replayed on open.
const DIRTY_PAGE_FLUSH_THRESHOLD: usize = 256;

/// Resident frames of one file's buffer. At the 4 KiB page size this bounds
/// one file's page cache at 2 MiB; the 256-page writeback threshold keeps a
/// clean-frame headroom so admission rarely waits on a dirty victim.
const TS_BUFFER_FRAMES: usize = 512;

/// Logical identity for one physical time-series file.
///
/// The relation layer assigns `file_index` values and WAL only works with this
/// numeric identity, never with `"key"` / `"value"` strings.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TimeSeriesFileIdentity {
    /// Partition owning the relation this file belongs to.
    pub partition_id: OID,
    /// Table the relation stores.
    pub table_id: OID,
    /// Relation-assigned file slot (key file, value file, ...).
    pub file_index: u32,
}

/// One record of a time-series file: its sort key, payload, and the physical
/// location it was read from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TimeSeriesRecord {
    /// Record timestamp (the primary sort key).
    pub timestamp: u64,
    /// Tuple id of the record (the secondary sort key).
    pub tuple_id: u64,
    /// Record payload bytes.
    pub payload: Vec<u8>,
    /// Page the record was read from.
    pub page_id: PageId,
    /// Slot index of the record inside `page_id`, in ascending key order.
    pub slot_index: usize,
}

fn raw_page_id(page_id: Option<PageId>) -> u64 {
    page_id
        .map(|id| id.as_u64())
        .unwrap_or(crate::page::page_header::NONE_PAGE_ID.as_u64())
}

fn page_id_from_raw(raw: u64) -> Option<PageId> {
    if raw == crate::page::page_header::NONE_PAGE_ID.as_u64() {
        None
    } else {
        Some(PageId::new(raw))
    }
}

/// A chain of slotted pages holding `(timestamp, tuple_id, payload)` records
/// in ascending key order, newest page at the head.
///
/// Writes go WAL-first: a mutation is planned in memory, persisted as a
/// physical-log batch, and only then published into the page buffer; the
/// data file itself is written later by eviction or writeback rounds. All
/// operations are routed through a mailbox to the thread-confined inner
/// object; on the owning worker thread an immediately-ready operation never
/// leaves the caller task.
pub struct TimeSeriesFile {
    mailbox: Mailbox<TsFileInner>,
    path: PathBuf,
    identity: Option<TimeSeriesFileIdentity>,
    // Cloned handle kept on the shell so the commit path's WAL flush does
    // not pay a mailbox hop. The backend is shared and internally
    // synchronized.
    wal_backend: Option<ChunkedWorkerLogBackend>,
}

impl TimeSeriesFile {
    pub(super) fn from_inner(
        inner: TsFileInner,
        path: PathBuf,
        identity: Option<TimeSeriesFileIdentity>,
        wal_backend: Option<ChunkedWorkerLogBackend>,
    ) -> RS<Self> {
        Ok(Self {
            mailbox: Mailbox::new(inner)?,
            path,
            identity,
            wal_backend,
        })
    }

    /// Identity and recovered WAL extent used to reject stale rebuildable indexes.
    pub(crate) async fn index_cache_stamp(&self) -> RS<Vec<u64>> {
        self.mailbox
            .call(|inner| Box::pin(async move { Ok(inner.index_cache_stamp()) }))
            .await
    }

    /// Returns the path of the data file.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Returns the logical identity of a relation-owned file, or `None` for a
    /// standalone file.
    pub fn identity(&self) -> Option<&TimeSeriesFileIdentity> {
        self.identity.as_ref()
    }

    #[cfg(test)]
    pub(crate) async fn page_count(&self) -> RS<PageId> {
        self.mailbox
            .call(|inner| Box::pin(async move { Ok(inner.page_count()) }))
            .await
    }

    #[cfg(test)]
    pub(crate) async fn head_page_id(&self) -> RS<Option<PageId>> {
        self.mailbox
            .call(|inner| Box::pin(async move { Ok(inner.head_page_id()) }))
            .await
    }

    #[cfg(test)]
    pub(crate) async fn tail_page_id(&self) -> RS<Option<PageId>> {
        self.mailbox
            .call(|inner| Box::pin(async move { Ok(inner.tail_page_id()) }))
            .await
    }

    /// Number of pages currently dirty (published but not yet written back
    /// to the data file).
    pub async fn dirty_page_count(&self) -> RS<usize> {
        self.mailbox
            .call(|inner| Box::pin(async move { Ok(inner.dirty_page_count()) }))
            .await
    }

    /// Looks up the record with exactly this `timestamp` and `tuple_id`,
    /// walking the page chain from the head (newest) page.
    pub async fn get(&self, timestamp: u64, tuple_id: u64) -> RS<Option<TimeSeriesRecord>> {
        self.mailbox
            .call(move |inner| Box::pin(async move { inner.get(timestamp, tuple_id).await }))
            .await
    }

    pub(crate) async fn scan_range(&self, begin_ts: u64, end_ts: u64) -> RS<Vec<TimeSeriesRecord>> {
        self.mailbox
            .call(move |inner| Box::pin(async move { inner.scan_range(begin_ts, end_ts).await }))
            .await
    }

    /// Inserts one record, allocating or splitting pages as needed. An
    /// existing `(timestamp, tuple_id)` key is updated in place.
    pub async fn insert(&self, timestamp: u64, tuple_id: u64, payload: &[u8]) -> RS<()> {
        let payload = payload.to_vec();
        self.mailbox
            .call(move |inner| {
                Box::pin(async move { inner.insert(timestamp, tuple_id, &payload).await })
            })
            .await
    }

    /// Inserts `rows` as one file mutation: every row is planned against an
    /// overlay carrying the previous rows' page images, then the merged plan
    /// is persisted with a single PL WAL append and applied once. Behavior
    /// per row is identical to calling [`TimeSeriesFile::insert`] for each
    /// row in order.
    pub(crate) async fn insert_batch(&self, rows: &[(u64, u64, &[u8])]) -> RS<()> {
        let rows: Vec<(u64, u64, Vec<u8>)> = rows
            .iter()
            .map(|(ts, id, payload)| (*ts, *id, payload.to_vec()))
            .collect();
        self.mailbox
            .call(move |inner| {
                Box::pin(async move {
                    let rows: Vec<(u64, u64, &[u8])> = rows
                        .iter()
                        .map(|(ts, id, payload)| (*ts, *id, payload.as_slice()))
                        .collect();
                    inner.insert_batch(&rows).await
                })
            })
            .await
    }

    /// Removes the record with exactly this `timestamp` and `tuple_id`.
    /// Returns whether such a record existed.
    pub async fn delete(&self, timestamp: u64, tuple_id: u64) -> RS<bool> {
        self.mailbox
            .call(move |inner| Box::pin(async move { inner.delete(timestamp, tuple_id).await }))
            .await
    }

    /// Writes back dirty data pages and then fsyncs the file. Tests that
    /// rely on `flush` making writes visible to a fresh open keep that
    /// semantic.
    pub async fn flush(&self) -> RS<()> {
        self.mailbox
            .call(|inner| Box::pin(async move { inner.flush().await }))
            .await
    }

    /// Writes back dirty data pages of this file without forcing durability;
    /// see [`TsFileInner::flush_dirty_pages`]. Used by the per-worker
    /// background flush driver.
    pub(crate) async fn flush_dirty_pages(&self) -> RS<()> {
        self.mailbox
            .call(|inner| Box::pin(async move { inner.flush_dirty_pages().await }))
            .await
    }

    /// Drives the WAL backend's group-commit queue to durable storage.
    /// Runs directly on the shell: the backend is shared and internally
    /// synchronized, so this needs no mailbox hop.
    pub(crate) async fn flush_wal_async(&self) -> RS<()> {
        if let Some(backend) = &self.wal_backend {
            backend.force_flush_log_async().await?;
            backend.fsync_unsynced_paths().await?;
        }
        Ok(())
    }

    /// Flushes the WAL group-commit queue and the dirty data pages, then
    /// closes the file. Writes are durable once this returns.
    pub async fn close(self) -> RS<()> {
        self.mailbox
            .call(|inner| Box::pin(async move { inner.close().await }))
            .await
    }

    /// Appends a delete record to the file's WAL (for a relation-owned
    /// file), then closes and removes the data file.
    pub async fn delete_file(self) -> RS<()> {
        self.mailbox
            .call(|inner| Box::pin(async move { inner.delete_file().await }))
            .await
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

mod delta_value;
mod inner;
