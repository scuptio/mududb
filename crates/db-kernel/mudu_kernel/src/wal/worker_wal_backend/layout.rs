use crate::wal::lsn::LSN;
use crate::wal::worker_log::scan_valid_frame_prefix;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_sys::contract::async_fs::AsyncFs;
use mudu_sys::fs::async_ as fs;
use short_uuid::ShortUuid;
use std::path::{Path, PathBuf};
use tracing::{debug, warn};
use uuid::Uuid;

use super::batching::WorkerLogBatching;
use super::sync_policy::WalSyncPolicy;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkerLogTail {
    pub current_sequence: Option<u64>,
    pub current_size: u64,
    pub next_sequence: u64,
    pub next_lsn: LSN,
}

#[derive(Clone, Debug)]
pub struct WorkerLogLayout {
    log_dir: PathBuf,
    pub(crate) log_oid: OID,
    chunk_size: u64,
    pub(crate) short_oid: String,
    batching: WorkerLogBatching,
    sync_policy: WalSyncPolicy,
}

impl Default for WorkerLogLayout {
    fn default() -> Self {
        Self::new_inner("", 0, 0)
    }
}

impl WorkerLogLayout {
    pub fn new_inner<P: Into<PathBuf>>(log_dir: P, log_oid: OID, chunk_size: u64) -> Self {
        Self {
            log_dir: log_dir.into(),
            log_oid,
            chunk_size,
            short_oid: ShortUuid::from_uuid(&Uuid::from_u128(log_oid)).to_string(),
            batching: WorkerLogBatching::default(),
            sync_policy: WalSyncPolicy::default(),
        }
    }

    pub fn is_invalid(&self) -> bool {
        self.log_oid == 0
    }

    pub fn new<P: Into<PathBuf>>(log_dir: P, log_oid: OID, chunk_size: u64) -> RS<Self> {
        if chunk_size == 0 {
            return Err(mudu_error!(
                ErrorCode::Parse,
                "worker log chunk size must be greater than zero"
            ));
        }
        Ok(Self::new_inner(log_dir, log_oid, chunk_size))
    }

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

    pub fn log_oid(&self) -> OID {
        self.log_oid
    }

    pub fn chunk_size(&self) -> u64 {
        self.chunk_size
    }

    pub fn chunk_path(&self, sequence: u64) -> PathBuf {
        self.log_dir
            .join(format!("{}.{}.xl", self.short_oid, sequence))
    }

    pub fn frame_size_limit(&self) -> usize {
        self.chunk_size as usize
    }

    pub fn batching(&self) -> WorkerLogBatching {
        self.batching
    }

    pub fn sync_policy(&self) -> WalSyncPolicy {
        self.sync_policy
    }

    pub async fn scan_tail(&self) -> RS<WorkerLogTail> {
        fs::create_dir_all(&self.log_dir).await?;
        let mut max_sequence: Option<u64> = None;
        for path in fs::read_dir(&self.log_dir).await? {
            if let Some(sequence) = self.parse_chunk_sequence(path.as_path()) {
                max_sequence = Some(max_sequence.map_or(sequence, |current| current.max(sequence)));
            }
        }
        let Some(sequence) = max_sequence else {
            return Ok(WorkerLogTail {
                current_sequence: None,
                current_size: 0,
                next_sequence: 0,
                next_lsn: LSN::new(0),
            });
        };
        let path = self.chunk_path(sequence);
        let size = fs::metadata_len(&path).await?;
        let next_lsn = self.scan_next_lsn().await?;
        if size < self.chunk_size {
            Ok(WorkerLogTail {
                current_sequence: Some(sequence),
                current_size: size,
                next_sequence: sequence + 1,
                next_lsn,
            })
        } else {
            Ok(WorkerLogTail {
                current_sequence: None,
                current_size: 0,
                next_sequence: sequence + 1,
                next_lsn,
            })
        }
    }

    pub async fn chunk_paths_sorted(&self) -> RS<Vec<PathBuf>> {
        let trace = mudu_utils::task_trace!();
        debug!("chunk_paths_sorted, begin, {}", self.log_oid);
        trace.watch("wal.layout.stage", "chunk_paths_sorted_create_dir");
        debug!("create_dir all, {}", self.log_oid);
        fs::create_dir_all(&self.log_dir).await?;
        debug!("create_dir all, end {}", self.log_oid);
        trace.watch("wal.layout.stage", "chunk_paths_sorted_read_dir");
        let mut entries = Vec::<(u64, PathBuf)>::new();
        for path in fs::read_dir(&self.log_dir).await? {
            debug!("read dir all, {}", self.log_oid);
            if let Some(sequence) = self.parse_chunk_sequence(path.as_path()) {
                entries.push((sequence, path));
            }
        }
        trace.watch("wal.layout.entries", &entries.len().to_string());
        entries.sort_by_key(|(sequence, _)| *sequence);
        debug!("chunk_paths_sorted, end, {}", self.log_oid);
        Ok(entries.into_iter().map(|(_, path)| path).collect())
    }

    fn parse_chunk_sequence(&self, path: &Path) -> Option<u64> {
        let file_name = path.file_name()?.to_str()?;
        let prefix = format!("{}.", self.short_oid);
        let suffix = ".xl";
        if !file_name.starts_with(&prefix) || !file_name.ends_with(suffix) {
            return None;
        }
        let sequence = &file_name[prefix.len()..file_name.len() - suffix.len()];
        sequence.parse::<u64>().ok()
    }

    async fn scan_next_lsn(&self) -> RS<LSN> {
        let mut max_lsn: Option<LSN> = None;
        for path in self.chunk_paths_sorted().await? {
            let bytes = fs::read_all(&path).await?;
            let prefix = scan_valid_frame_prefix(&bytes);
            if let Some(reason) = &prefix.corrupt_reason {
                warn!(
                    path = %path.display(),
                    truncated_bytes = bytes.len() - prefix.valid_len,
                    reason = %reason,
                    "worker log chunk has an un-persisted tail"
                );
            }
            if let Some(lsn) = prefix.max_lsn {
                max_lsn = Some(max_lsn.map_or(lsn, |current| current.max(lsn)));
            }
        }
        Ok(max_lsn.map_or(LSN::new(0), |lsn| lsn.saturating_add(1)))
    }

    pub async fn scan_tail_async(&self, fs: &dyn AsyncFs) -> RS<WorkerLogTail> {
        mudu_utils::scoped_task_trace!();
        fs.create_dir_all(&self.log_dir).await?;
        let sequences = self.chunk_sequences_async(fs).await?;
        let max_sequence = sequences.last().copied();
        let Some(sequence) = max_sequence else {
            return Ok(WorkerLogTail {
                current_sequence: None,
                current_size: 0,
                next_sequence: 0,
                next_lsn: LSN::new(0),
            });
        };
        // Walk every chunk once: derive the next LSN from the longest valid
        // frame prefix of each chunk, and truncate any un-persisted or
        // corrupt tail so appends resume at a valid frame boundary. A crash
        // can leave such a tail behind (write() without a completed fsync,
        // or a torn final frame); the frames carry CRCs, so everything up to
        // the first invalid frame is trustworthy.
        let mut max_lsn: Option<LSN> = None;
        let mut tail_size = 0u64;
        for chunk_sequence in &sequences {
            let path = self.chunk_path(*chunk_sequence);
            let bytes = fs.read_all(&path).await?;
            let prefix = scan_valid_frame_prefix(&bytes);
            let mut size = bytes.len() as u64;
            if let Some(reason) = &prefix.corrupt_reason {
                let truncated_bytes = bytes.len() - prefix.valid_len;
                warn!(
                    path = %path.display(),
                    truncated_bytes,
                    reason = %reason,
                    "truncating un-persisted worker log chunk tail"
                );
                truncate_file_to(&path, prefix.valid_len as u64)?;
                size = prefix.valid_len as u64;
            }
            if let Some(lsn) = prefix.max_lsn {
                max_lsn = Some(max_lsn.map_or(lsn, |current| current.max(lsn)));
            }
            if *chunk_sequence == sequence {
                tail_size = size;
            }
        }
        let next_lsn = max_lsn.map_or(LSN::new(0), |lsn| lsn.saturating_add(1));
        if tail_size < self.chunk_size {
            Ok(WorkerLogTail {
                current_sequence: Some(sequence),
                current_size: tail_size,
                next_sequence: sequence + 1,
                next_lsn,
            })
        } else {
            Ok(WorkerLogTail {
                current_sequence: None,
                current_size: 0,
                next_sequence: sequence + 1,
                next_lsn,
            })
        }
    }

    pub async fn chunk_paths_sorted_async(&self, fs: &dyn AsyncFs) -> RS<Vec<PathBuf>> {
        fs.create_dir_all(&self.log_dir).await?;
        let mut entries = Vec::<(u64, PathBuf)>::new();
        for path in fs.read_dir(&self.log_dir).await? {
            if let Some(sequence) = self.parse_chunk_sequence(path.as_path()) {
                entries.push((sequence, path));
            }
        }
        entries.sort_by_key(|(sequence, _)| *sequence);
        Ok(entries.into_iter().map(|(_, path)| path).collect())
    }

    async fn chunk_sequences_async(&self, fs: &dyn AsyncFs) -> RS<Vec<u64>> {
        let trace = mudu_utils::task_trace!();
        trace.watch("wal.layout.stage", "chunk_sequences_start");
        let mut sequences = Vec::new();
        let mut sequence = 0u64;
        loop {
            trace.watch("wal.layout.sequence_probe", &sequence.to_string());
            let path = self.chunk_path(sequence);
            if !fs.path_exists(&path).await? {
                trace.watch("wal.layout.stage", "chunk_sequences_done");
                break;
            }
            sequences.push(sequence);
            sequence = sequence.saturating_add(1);
        }
        trace.watch("wal.layout.sequences", &sequences.len().to_string());
        Ok(sequences)
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
