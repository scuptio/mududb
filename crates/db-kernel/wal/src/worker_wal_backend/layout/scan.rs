use super::*;
use crate::lsn::LSN;
use crate::worker_log::scan_valid_frame_prefix;
use mudu_sys::contract::async_fs::AsyncFs;
use mudu_sys::fs::async_ as fs;
use std::path::{Path, PathBuf};
use tracing::{debug, warn};

impl WorkerLogLayout {
    /// Scans the log directory through the default async filesystem and
    /// returns the append tail position.
    pub async fn scan_tail(&self) -> RS<WorkerLogTail> {
        fs::create_dir_all(&self.log_dir).await?;
        let mut max_sequence: Option<u64> = None;
        for path in fs::read_dir(&self.log_dir).await? {
            if let Some(sequence) = self.parse_chunk_sequence(path.as_path()) {
                max_sequence = Some(max_sequence.map_or(sequence, |current| current.max(sequence)));
            }
        }
        let Some(sequence) = max_sequence else {
            let frontier = self.gc_frontier_or_zero();
            return Ok(WorkerLogTail {
                current_sequence: None,
                current_size: 0,
                next_sequence: frontier.first_alive_seq,
                next_lsn: LSN::new(frontier.lsn),
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

    /// Chunk paths in chunk-sequence order, listed through the default async
    /// filesystem.
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

    pub(super) fn parse_chunk_sequence(&self, path: &Path) -> Option<u64> {
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

    /// Scans the log directory through `fs`, truncating any un-persisted or
    /// corrupt chunk tail, and returns the append tail position.
    pub async fn scan_tail_async(&self, fs: &dyn AsyncFs) -> RS<WorkerLogTail> {
        mudu_utils::scoped_task_trace!();
        fs.create_dir_all(&self.log_dir).await?;
        let sequences = self.chunk_sequences_async(fs).await?;
        let max_sequence = sequences.last().copied();
        let Some(sequence) = max_sequence else {
            // Every chunk was reclaimed by prefix GC: the allocator resumes
            // at the persisted frontier, never at zero.
            let frontier = self.gc_frontier_or_zero();
            return Ok(WorkerLogTail {
                current_sequence: None,
                current_size: 0,
                next_sequence: frontier.first_alive_seq,
                next_lsn: LSN::new(frontier.lsn),
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

    /// Chunk paths in chunk-sequence order, listed through `fs`.
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
        // Prefix GC leaves holes: probing from sequence 0 would stop at the
        // first missing chunk and mistake the log for empty, resetting the
        // LSN allocator. The persisted frontier says where the log starts.
        let frontier = self.gc_frontier_or_zero();
        let mut sequences = Vec::new();
        let mut sequence = frontier.first_alive_seq;
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
