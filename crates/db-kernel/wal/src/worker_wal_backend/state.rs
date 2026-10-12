use mudu::common::result::RS;
use mudu_sys::fs::SysFile;
use std::path::{Path, PathBuf};

use super::layout::{WorkerLogLayout, WorkerLogTail};

/// Mutable append state of a chunked worker log: which chunk is the append
/// tail, how full it is, and which sequence the next chunk takes.
pub struct ChunkedWorkerLog {
    /// On-disk layout this state appends into.
    pub layout: WorkerLogLayout,
    current_sequence: Option<u64>,
    current_size: u64,
    current_file: Option<(PathBuf, SysFile)>,
    // next chunk sequence
    next_sequence: u64,
}

/// A reserved append position: the chunk file, the byte offset of the write,
/// and whether the chunk must be flushed right after it.
pub struct AppendReservation {
    /// Chunk file to write into.
    pub path: PathBuf,
    /// Byte offset of the write within the chunk.
    pub offset: u64,
    /// Whether the chunk must be flushed immediately after this write (an
    /// oversized payload that gets its own chunk).
    pub flush_after_write: bool,
}

impl ChunkedWorkerLog {
    /// Creates the append state from a scanned append tail.
    pub fn new(layout: WorkerLogLayout, tail: WorkerLogTail) -> RS<Self> {
        Ok(Self {
            layout,
            current_sequence: tail.current_sequence,
            current_size: tail.current_size,
            current_file: None,
            next_sequence: tail.next_sequence,
        })
    }

    pub(crate) fn reserve_append(&mut self, payload_len: u64) -> RS<AppendReservation> {
        if payload_len == 0 {
            return Ok(AppendReservation {
                path: self
                    .layout
                    .chunk_path(self.current_sequence.unwrap_or(self.next_sequence)),
                offset: self.current_size,
                flush_after_write: false,
            });
        }

        if payload_len > self.layout.chunk_size() {
            let sequence = self.next_sequence;
            self.next_sequence += 1;
            self.current_sequence = None;
            self.current_size = 0;
            return Ok(AppendReservation {
                path: self.layout.chunk_path(sequence),
                offset: 0,
                flush_after_write: true,
            });
        }

        if self.current_sequence.is_none()
            || self.current_size + payload_len > self.layout.chunk_size()
        {
            self.current_sequence = Some(self.next_sequence);
            self.current_size = 0;
            self.next_sequence += 1;
        }

        #[expect(clippy::expect_used, reason = "current_sequence is set to Some above")]
        let sequence = self.current_sequence.expect("current sequence must exist");
        let offset = self.current_size;
        self.current_size += payload_len;
        if self.current_size >= self.layout.chunk_size() {
            self.current_sequence = None;
            self.current_size = 0;
        }
        Ok(AppendReservation {
            path: self.layout.chunk_path(sequence),
            offset,
            flush_after_write: false,
        })
    }

    pub(crate) fn current_path(&self) -> Option<PathBuf> {
        self.current_sequence
            .map(|sequence| self.layout.chunk_path(sequence))
    }

    /// Resets the append tail after an external truncation: appends resume
    /// at `(current_sequence, current_size)` and new chunks number from
    /// `next_sequence`.
    pub(crate) fn reset_tail(
        &mut self,
        current_sequence: Option<u64>,
        current_size: u64,
        next_sequence: u64,
    ) {
        self.current_sequence = current_sequence;
        self.current_size = current_size;
        self.next_sequence = next_sequence;
        self.current_file = None;
    }

    pub(crate) fn take_current_file(&mut self, path: &Path) -> Option<SysFile> {
        let (cached_path, file) = self.current_file.take()?;
        if cached_path == path {
            Some(file)
        } else {
            self.current_file = Some((cached_path, file));
            None
        }
    }

    pub(crate) fn store_current_file(&mut self, path: &Path, file: SysFile) -> Option<SysFile> {
        let Some(current_path) = self.current_path() else {
            return Some(file);
        };
        if current_path != path {
            return Some(file);
        }
        let replaced = self.current_file.take().map(|(_, file)| file);
        self.current_file = Some((path.to_path_buf(), file));
        replaced
    }
}
