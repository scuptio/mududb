use crate::wal::log_frame::{frame_len, split_frame};
pub use crate::wal::worker_wal_backend::{
    WorkerLogBatching, WorkerLogLayout, WorkerLogTail, WorkerWALBackend as ChunkedWorkerLogBackend,
};
use async_trait::async_trait;
use mudu::common::result::RS;
use serde::Serialize;
use std::path::{Path, PathBuf};

#[async_trait]
pub trait WorkerLogBackend: Clone + Send + Sync {
    fn frame_size_limit(&self) -> RS<usize>;

    fn serialize_entry<L: Serialize + Send + Sync>(&self, entry: &L) -> RS<Vec<Vec<u8>>>;
    async fn chunk_paths_sorted(&self) -> RS<Vec<PathBuf>>;
    async fn append_frames_async(&self, frames: Vec<Vec<u8>>) -> RS<()>;

    fn flush(&self) -> RS<()>;
    async fn flush_async(&self) -> RS<()>;
}

#[async_trait]
pub trait WorkerLogRecoverySource {
    async fn chunk_paths_sorted(&self) -> RS<Vec<PathBuf>>;
    async fn read_chunk(&self, path: &Path) -> RS<Vec<u8>>;
}

#[async_trait]
pub trait AsyncWorkerLogRecoverySource: Send {
    async fn chunk_paths_sorted(&mut self) -> RS<Vec<PathBuf>>;
    async fn read_chunk(&mut self, path: &Path) -> RS<Vec<u8>>;
}

pub fn decode_frames(payload: &[u8]) -> RS<Vec<Vec<u8>>> {
    let mut offset = 0usize;
    let mut frames = Vec::new();
    while offset < payload.len() {
        let remaining = &payload[offset..];
        let next_frame_len = frame_len(remaining)?;
        let frame = &remaining[..next_frame_len];
        split_frame(frame)?;
        frames.push(frame.to_vec());
        offset += next_frame_len;
    }
    Ok(frames)
}

/// Decode complete log frames and silently drop a trailing partial frame.
///
/// WAL chunks may contain an incomplete final frame if the writer was
/// interrupted before the frame could be fully persisted. Recovery should
/// ignore such trailing bytes rather than fail.
pub fn decode_frames_allow_trailing(payload: &[u8]) -> RS<Vec<Vec<u8>>> {
    use crate::wal::format::latest::{LOG_FRAME_HEADER_SIZE, LOG_FRAME_TAILER_SIZE};
    let mut offset = 0usize;
    let mut frames = Vec::new();
    while offset < payload.len() {
        let remaining = &payload[offset..];
        let next_frame_len = match frame_len(remaining) {
            Ok(len) => len,
            Err(_) if remaining.len() < LOG_FRAME_HEADER_SIZE + LOG_FRAME_TAILER_SIZE => break,
            Err(e) => return Err(e),
        };
        let frame = &remaining[..next_frame_len];
        split_frame(frame)?;
        frames.push(frame.to_vec());
        offset += next_frame_len;
    }
    Ok(frames)
}
