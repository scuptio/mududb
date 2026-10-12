use super::*;
use crate::worker_log::scan_valid_frame_prefix;
use mudu_sys::contract::async_fs::AsyncFs;
use mudu_sys::contract::file_options::FileOptions;
use std::path::{Path, PathBuf};

impl WorkerLogLayout {
    /// Path a chunk is copied to inside the archive dir. The archived chunk
    /// keeps the live filename (`{short_oid}.{sequence}.xl`) so a restore can
    /// replay it without renaming.
    pub fn archived_chunk_path(&self, sequence: u64) -> RS<PathBuf> {
        let dir = self.archive_dir.as_ref().ok_or_else(|| {
            mudu_error!(ErrorCode::InvalidArgument, "archive dir is not configured")
        })?;
        Ok(dir.join(format!("{}.{}.xl", self.short_oid, sequence)))
    }

    /// Copies every sealed chunk (every chunk except the append tail) up to
    /// the current live end into the archive dir, fsyncs each copy, verifies
    /// its length against the source, and advances the persisted archive
    /// frontier. Returns `(archived bytes, archived chunks)`.
    ///
    /// The append tail (the last chunk in sequence order) is never archived:
    /// it may still be growing, so its frames are not yet sealed.
    pub async fn archive_prefix_chunks(&self, fs: &dyn AsyncFs) -> RS<(u64, u64)> {
        if self.archive_dir.is_none() {
            return Ok((0, 0));
        }
        let existing = self.archive_frontier_or_zero();
        let chunks = self.chunk_paths_sorted_async(fs).await?;
        let mut frontier = existing;
        let mut archived_bytes = 0u64;
        let mut archived_chunks = 0u64;
        for (index, path) in chunks.iter().enumerate() {
            // Never archive the append tail.
            if index + 1 == chunks.len() {
                break;
            }
            let Some(sequence) = self.parse_chunk_sequence(path) else {
                continue;
            };
            if sequence < existing.first_unarchived_seq {
                continue;
            }
            let dest = self.archived_chunk_path(sequence)?;
            let bytes = fs.read_all(path).await?;
            if bytes.is_empty() {
                continue;
            }
            if let Some(parent) = dest.parent() {
                fs.create_dir_all(parent).await?;
            }
            let file = fs.open(&dest, FileOptions::read_write_create()).await?;
            file.write_all_at(0, &bytes).await?;
            file.fsync().await?;
            // Verify the copy before declaring it archived: a torn write must
            // not poison the frontier.
            let written = fs.metadata_len(&dest).await?;
            if written != bytes.len() as u64 {
                return Err(mudu_error!(
                    ErrorCode::Io,
                    format!(
                        "archived chunk {} length mismatch: source {}, written {}",
                        dest.display(),
                        bytes.len(),
                        written
                    )
                ));
            }
            let max_lsn = self.chunk_max_lsn(fs, path).await?;
            frontier.lsn = max_lsn.map_or(frontier.lsn, |lsn| lsn.saturating_add(1));
            frontier.first_unarchived_seq = sequence.saturating_add(1);
            archived_bytes += bytes.len() as u64;
            archived_chunks += 1;
        }
        if archived_chunks > 0 {
            self.store_archive_frontier(frontier)?;
        }
        Ok((archived_bytes, archived_chunks))
    }

    /// The highest frame LSN in a chunk (`None` when the chunk holds no valid
    /// frames). Shared by GC and archiving.
    async fn chunk_max_lsn(&self, fs: &dyn AsyncFs, path: &Path) -> RS<Option<u64>> {
        let bytes = fs.read_all(path).await?;
        if bytes.is_empty() {
            return Ok(None);
        }
        let prefix = scan_valid_frame_prefix(&bytes);
        Ok(prefix.max_lsn.map(|lsn| lsn.as_u64()))
    }
}
