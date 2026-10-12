use super::*;
use crate::worker_log::scan_valid_frame_prefix;
use mudu_sys::contract::async_fs::AsyncFs;
use std::path::PathBuf;

impl WorkerLogLayout {
    /// Deletes every chunk whose frames all lie below `frontier_lsn`
    /// (the append tail is never deleted) and persists the advanced GC
    /// frontier marker. Returns `(reclaimed bytes, reclaimed chunks)`.
    pub async fn gc_prefix_chunks(&self, fs: &dyn AsyncFs, frontier_lsn: u64) -> RS<(u64, u64)> {
        let existing = self.gc_frontier_or_zero();
        let frontier_lsn = frontier_lsn.max(existing.lsn);
        // Archive guard: with archiving enabled, never reclaim a chunk the
        // archiver has not yet copied (the archive frontier is the first LSN
        // not yet archived).
        let frontier_lsn = if self.archive_dir.is_some() {
            frontier_lsn.min(self.archive_frontier_or_zero().lsn)
        } else {
            frontier_lsn
        };
        let chunks = self.chunk_paths_sorted_async(fs).await?;
        let mut deletable: Vec<(u64, PathBuf, u64)> = Vec::new();
        for (index, path) in chunks.iter().enumerate() {
            // The last chunk is the append tail: never reclaimed here.
            if index + 1 == chunks.len() {
                break;
            }
            let bytes = fs.read_all(path).await?;
            if bytes.is_empty() {
                continue;
            }
            let prefix = scan_valid_frame_prefix(&bytes);
            let Some(max_lsn) = prefix.max_lsn else {
                continue;
            };
            if max_lsn.as_u64() < frontier_lsn {
                let sequence = self.parse_chunk_sequence(path).ok_or_else(|| {
                    mudu_error!(
                        ErrorCode::Internal,
                        format!("worker log chunk {} has no sequence", path.display())
                    )
                })?;
                deletable.push((sequence, path.clone(), bytes.len() as u64));
            }
        }
        if deletable.is_empty() {
            return Ok((0, 0));
        }
        let first_alive_seq = deletable
            .last()
            .map(|(sequence, _, _)| sequence + 1)
            .unwrap_or(0);
        self.store_gc_frontier(GcFrontier {
            lsn: frontier_lsn,
            first_alive_seq,
        })?;
        let mut freed = 0u64;
        let mut chunks = 0u64;
        for (_, path, len) in deletable {
            fs.remove_file_if_exists(&path).await?;
            freed += len;
            chunks += 1;
        }
        Ok((freed, chunks))
    }
}
