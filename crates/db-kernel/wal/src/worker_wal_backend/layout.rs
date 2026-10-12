use crate::lsn::LSN;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use serde::{Deserialize, Serialize};
use short_uuid::ShortUuid;
use std::path::{Path, PathBuf};
use uuid::Uuid;

use super::batching::WorkerLogBatching;
use super::sync_policy::WalSyncPolicy;

/// Persisted GC frontier of a worker log (`<short_oid>.gcfrontier`):
/// `lsn` is the first LSN still present — every frame below it was
/// reclaimed — and `first_alive_seq` is the chunk sequence the enumeration
/// starts from (prefix deletion leaves holes, so the legacy probe from
/// sequence 0 would mistake the log for empty and reset the LSN allocator).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GcFrontier {
    /// First LSN still present in the log.
    pub lsn: u64,
    /// First chunk sequence still present (next sequence when empty).
    pub first_alive_seq: u64,
}

const GC_FRONTIER_SUFFIX: &str = ".gcfrontier";

/// Persisted archive frontier of a worker log (`<short_oid>.archfrontier`):
/// `lsn` is the first LSN not yet archived, and `first_unarchived_seq` is
/// the chunk sequence the archiver resumes from. WAL prefix GC must never
/// reclaim a chunk at or above this frontier (an un-archived chunk cannot
/// be deleted).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArchiveFrontier {
    /// First LSN not yet archived.
    pub lsn: u64,
    /// First chunk sequence not yet archived.
    pub first_unarchived_seq: u64,
}

const ARCHIVE_FRONTIER_SUFFIX: &str = ".archfrontier";

/// Scan result describing where a worker log's append tail resumes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkerLogTail {
    /// Sequence of the chunk still accepting appends; `None` when the tail
    /// chunk is full or the log is empty.
    pub current_sequence: Option<u64>,
    /// Bytes already written to the current chunk.
    pub current_size: u64,
    /// Sequence the next new chunk will take.
    pub next_sequence: u64,
    /// Next LSN the log allocates (one past the highest frame found).
    pub next_lsn: LSN,
}

/// On-disk layout of one worker log: directory, identity, chunk size,
/// batching watermarks, durability policy, and optional archive destination.
#[derive(Clone, Debug)]
pub struct WorkerLogLayout {
    log_dir: PathBuf,
    /// Object identifier of the worker log.
    pub log_oid: OID,
    chunk_size: u64,
    /// Short text form of `log_oid`, used as the file-name prefix of chunk
    /// files and frontier markers.
    pub short_oid: String,
    batching: WorkerLogBatching,
    sync_policy: WalSyncPolicy,
    /// When set, WAL chunks are continuously archived to this directory and
    /// prefix GC is capped so an un-archived chunk is never reclaimed.
    archive_dir: Option<PathBuf>,
}

impl Default for WorkerLogLayout {
    fn default() -> Self {
        Self::new_inner("", 0, 0)
    }
}

impl WorkerLogLayout {
    /// Builds a layout without validating `chunk_size` (used by `Default` for
    /// the zero placeholder).
    pub fn new_inner<P: Into<PathBuf>>(log_dir: P, log_oid: OID, chunk_size: u64) -> Self {
        Self {
            log_dir: log_dir.into(),
            log_oid,
            chunk_size,
            short_oid: ShortUuid::from_uuid(&Uuid::from_u128(log_oid)).to_string(),
            batching: WorkerLogBatching::default(),
            sync_policy: WalSyncPolicy::default(),
            archive_dir: None,
        }
    }

    /// True for the zero placeholder layout (log OID 0).
    pub fn is_invalid(&self) -> bool {
        self.log_oid == 0
    }

    /// Builds a layout; `chunk_size` must be greater than zero.
    pub fn new<P: Into<PathBuf>>(log_dir: P, log_oid: OID, chunk_size: u64) -> RS<Self> {
        if chunk_size == 0 {
            return Err(mudu_error!(
                ErrorCode::Parse,
                "worker log chunk size must be greater than zero"
            ));
        }
        Ok(Self::new_inner(log_dir, log_oid, chunk_size))
    }

    /// Overrides the flush-batching watermarks.
    pub fn with_batching(mut self, batching: WorkerLogBatching) -> Self {
        self.batching = batching;
        self
    }

    /// Overrides the WAL durability policy (see [`WalSyncPolicy`]). Defaults
    /// to [`WalSyncPolicy::Commit`].
    pub fn with_sync_policy(mut self, sync_policy: WalSyncPolicy) -> Self {
        self.sync_policy = sync_policy;
        self
    }

    /// Enables continuous WAL archiving to `archive_dir`. When set, prefix GC
    /// is capped so a chunk is never reclaimed before it is archived.
    pub fn with_archive_dir<P: Into<PathBuf>>(mut self, archive_dir: P) -> Self {
        self.archive_dir = Some(archive_dir.into());
        self
    }

    /// The configured archive directory, when archiving is enabled.
    pub fn archive_dir(&self) -> Option<&Path> {
        self.archive_dir.as_deref()
    }

    /// Object identifier of the worker log.
    pub fn log_oid(&self) -> OID {
        self.log_oid
    }

    /// Maximum byte size of one chunk file.
    pub fn chunk_size(&self) -> u64 {
        self.chunk_size
    }

    /// Path of chunk `sequence` (`{short_oid}.{sequence}.xl`).
    pub fn chunk_path(&self, sequence: u64) -> PathBuf {
        self.log_dir
            .join(format!("{}.{}.xl", self.short_oid, sequence))
    }

    /// Maximum byte size of one encoded frame (the chunk size).
    pub fn frame_size_limit(&self) -> usize {
        self.chunk_size as usize
    }

    /// The configured flush-batching watermarks.
    pub fn batching(&self) -> WorkerLogBatching {
        self.batching
    }

    /// The configured durability policy.
    pub fn sync_policy(&self) -> WalSyncPolicy {
        self.sync_policy
    }
}

/// Truncates `path` to `len` bytes and persists the size change, so a
/// dropped un-persisted WAL tail cannot reappear after another crash.
fn truncate_file_to(path: &Path, len: u64) -> RS<()> {
    let mut options = mudu_sys::fs::sync::SOpenOptions::new();
    options.read(true).write(true);
    let file = options.open(path)?;
    file.set_len(len)?;
    file.sync_data()?;
    Ok(())
}

mod archive;
mod frontier;
mod gc;
mod scan;
mod truncate;
