use crate::wal::log_frame::{frame_len, split_frame};
use crate::wal::lsn::LSN;
pub use crate::wal::worker_wal_backend::{
    WorkerLogBatching, WorkerLogLayout, WorkerLogTail, WorkerWALBackend as ChunkedWorkerLogBackend,
};
use async_trait::async_trait;
use mudu::common::result::RS;
use serde::Serialize;
use std::path::{Path, PathBuf};

#[async_trait]
pub trait WorkerLogBackend: Clone + Send + Sync + 'static {
    fn frame_size_limit(&self) -> RS<usize>;

    fn serialize_entry<L: Serialize + Send + Sync>(&self, entry: &L) -> RS<Vec<Vec<u8>>>;
    fn chunk_paths_sorted(&self) -> RS<Vec<PathBuf>>;
    fn append_frames_sync(&self, frames: Vec<Vec<u8>>) -> RS<()>;
    async fn append_frames_async(&self, frames: Vec<Vec<u8>>) -> RS<LSN>;
    fn flush(&self) -> RS<()>;
    async fn flush_async(&self) -> RS<()>;
}

pub trait WorkerLogRecoverySource {
    fn chunk_paths_sorted(&mut self) -> RS<Vec<PathBuf>>;
    fn read_chunk(&mut self, path: &Path) -> RS<Vec<u8>>;
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
