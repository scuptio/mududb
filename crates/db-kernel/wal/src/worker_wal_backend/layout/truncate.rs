use super::*;
use crate::log_frame::{frame_len, split_frame};
use mudu_sys::contract::async_fs::AsyncFs;
use tracing::warn;

impl WorkerLogLayout {
    /// Cuts every chunk content at and after `from_lsn`: truncates the chunk
    /// containing the first such frame at that frame's offset and deletes
    /// all later chunks. Returns the cut point `(chunk_sequence, offset)`
    /// when a cut happened, `None` when the log already ends before
    /// `from_lsn`.
    ///
    /// Used by replication healing to drop the uncommitted tail of a former
    /// leader: frames flushed but never raft-committed must not be replayed
    /// or extended. Callers running a live backend must reset its state
    /// afterwards (see `WorkerWALBackend::truncate_from_lsn`).
    pub async fn truncate_chunks_from_lsn(
        &self,
        fs: &dyn AsyncFs,
        from_lsn: u64,
    ) -> RS<Option<(u64, u64)>> {
        let chunks = self.chunk_paths_sorted_async(fs).await?;
        for (index, path) in chunks.iter().enumerate() {
            let bytes = fs.read_all(path).await?;
            if bytes.is_empty() {
                continue;
            }
            let mut offset = 0usize;
            while offset < bytes.len() {
                let Ok(len) = frame_len(&bytes[offset..]) else {
                    break;
                };
                let frame = &bytes[offset..offset + len];
                let lsn = match split_frame(frame) {
                    Ok((header, _, _)) => header.lsn().as_u64(),
                    Err(_) => break,
                };
                if lsn >= from_lsn {
                    let sequence = self.parse_chunk_sequence(path).ok_or_else(|| {
                        mudu_error!(
                            ErrorCode::Internal,
                            format!("worker log chunk {} has no sequence", path.display())
                        )
                    })?;
                    warn!(
                        path = %path.display(),
                        from_lsn, "truncating uncommitted worker log tail"
                    );
                    truncate_file_to(path, offset as u64)?;
                    for later in &chunks[index + 1..] {
                        fs.remove_file_if_exists(later).await?;
                    }
                    return Ok(Some((sequence, offset as u64)));
                }
                offset += len;
            }
        }
        Ok(None)
    }
}
