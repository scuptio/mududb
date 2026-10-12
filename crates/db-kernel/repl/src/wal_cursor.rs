//! Monotonic frame cursor over a worker log's chunk files.
//!
//! The commit sequencer (segment hashing on the leader) and the promote
//! applier (chain recomputation) both walk the local worker log strictly in
//! LSN order. The cursor re-lists chunks every round (the flush driver may
//! have rotated), tolerates a torn tail on the last chunk (the writer may
//! complete it later) and never advances past an invalid frame.

use ::wal::worker_log::{scan_valid_frame_prefix, ChunkedWorkerLogBackend, WorkerLogBackend};
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_sys::contract::async_file::AsyncFile;
use mudu_sys::contract::file_options::FileOptions;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Bytes read from the current chunk in one step; whole-frame reads always
/// complete because a frame never exceeds a chunk.
const CURSOR_READ_SLICE: u64 = 1024 * 1024;

/// Sequential reader over one worker log's chunk files.
pub(crate) struct LogFrameCursor {
    log: ChunkedWorkerLogBackend,
    current_path: Option<PathBuf>,
    offset: u64,
    open_file: Option<(PathBuf, Arc<dyn AsyncFile>)>,
    buffered: VecDeque<Vec<u8>>,
}

impl LogFrameCursor {
    /// Starts a cursor at the beginning of the log.
    pub fn new(log: ChunkedWorkerLogBackend) -> Self {
        Self {
            log,
            current_path: None,
            offset: 0,
            open_file: None,
            buffered: VecDeque::new(),
        }
    }

    /// Returns the next frame in log order, or `None` when the cursor stands
    /// at the current end of the log (a torn tail on the last chunk counts
    /// as end; the caller may retry after the writer completes it).
    pub(crate) async fn next_frame(&mut self) -> RS<Option<Vec<u8>>> {
        loop {
            if let Some(frame) = self.buffered.pop_front() {
                return Ok(Some(frame));
            }
            if !self.fill_buffer().await? {
                return Ok(None);
            }
        }
    }

    /// Reads the next chunk slice and buffers its frames; `Ok(false)` at the
    /// current end of the log.
    async fn fill_buffer(&mut self) -> RS<bool> {
        loop {
            let chunks = self.log.chunk_paths_sorted().await?;
            let Some(chunk_idx) = self.current_chunk_index(&chunks)? else {
                return Ok(false);
            };
            let path = chunks[chunk_idx].clone();
            let file = self.open_current(&path).await?;
            let file_len = file.file_len().await?;
            if self.offset >= file_len {
                // Current chunk exhausted; only a later chunk can hold more.
                if chunk_idx + 1 < chunks.len() {
                    self.current_path = Some(chunks[chunk_idx + 1].clone());
                    self.offset = 0;
                    self.open_file = None;
                    continue;
                }
                return Ok(false);
            }
            let read_len = (file_len - self.offset).min(CURSOR_READ_SLICE);
            let mut bytes = file.read_exact_at(self.offset, read_len as usize).await?;
            // A single frame may exceed the slice; grow the window until one
            // whole frame fits or the chunk end is reached.
            let mut read_len = read_len;
            while scan_valid_frame_prefix(&bytes).frames.is_empty()
                && read_len < file_len - self.offset
            {
                read_len = (read_len.saturating_mul(2)).min(file_len - self.offset);
                bytes = file.read_exact_at(self.offset, read_len as usize).await?;
            }
            let prefix = scan_valid_frame_prefix(&bytes);
            if prefix.frames.is_empty() {
                // Torn tail on the last chunk: treat as end-of-log for now.
                if chunk_idx + 1 < chunks.len() {
                    self.current_path = Some(chunks[chunk_idx + 1].clone());
                    self.offset = 0;
                    self.open_file = None;
                    continue;
                }
                return Ok(false);
            }
            self.offset += prefix.valid_len as u64;
            self.buffered = prefix.frames.into();
            return Ok(true);
        }
    }

    fn current_chunk_index(&self, chunks: &[PathBuf]) -> RS<Option<usize>> {
        match &self.current_path {
            None => Ok(if chunks.is_empty() { None } else { Some(0) }),
            Some(path) => {
                let index = chunks.iter().position(|candidate| candidate == path);
                if index.is_none() {
                    return Err(mudu_error!(
                        ErrorCode::EntityNotFound,
                        format!("worker log chunk {} disappeared", path.display())
                    ));
                }
                Ok(index)
            }
        }
    }

    async fn open_current(&mut self, path: &Path) -> RS<Arc<dyn AsyncFile>> {
        if let Some((open_path, file)) = &self.open_file {
            if open_path == path {
                return Ok(file.clone());
            }
        }
        let file = self.log.fs().open(path, FileOptions::read_only()).await?;
        self.open_file = Some((path.to_path_buf(), file.clone()));
        Ok(file)
    }
}
