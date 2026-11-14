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
        let plan = super::append_plan::reserve(
            super::append_plan::AppendState {
                current: self.current_sequence,
                size: self.current_size,
                next: self.next_sequence,
            },
            self.layout.chunk_size(),
            payload_len,
        )
        .ok_or_else(|| {
            mudu::mudu_error!(
                mudu::error::ErrorCode::Internal,
                "WAL chunk sequence exhausted"
            )
        })?;
        // Publish only a complete reservation; exhaustion leaves the cursor untouched.
        self.current_sequence = plan.state.current;
        self.current_size = plan.state.size;
        self.next_sequence = plan.state.next;
        Ok(AppendReservation {
            path: self.layout.chunk_path(plan.sequence),
            offset: plan.offset,
            flush_after_write: plan.flush,
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

#[cfg(test)]
mod reservation_tests {
    use super::*;

    #[test]
    fn rollover_oversized_and_zero_length_reservations() -> RS<()> {
        let layout = WorkerLogLayout::new("unused", 1, 10)?;
        let mut log = ChunkedWorkerLog::new(
            layout.clone(),
            WorkerLogTail {
                current_sequence: None,
                current_size: 0,
                next_sequence: 0,
                next_lsn: 0u64.into(),
            },
        )?;
        let first = log.reserve_append(6)?;
        let second = log.reserve_append(4)?;
        assert_eq!((first.path, first.offset), (layout.chunk_path(0), 0));
        assert_eq!((second.path, second.offset), (layout.chunk_path(0), 6));
        let empty = log.reserve_append(0)?;
        assert_eq!((empty.path, empty.offset), (layout.chunk_path(1), 0));
        let oversized = log.reserve_append(11)?;
        assert_eq!(
            (oversized.path, oversized.offset),
            (layout.chunk_path(1), 0)
        );
        assert!(oversized.flush_after_write);
        let next = log.reserve_append(2)?;
        assert_eq!((next.path, next.offset), (layout.chunk_path(2), 0));
        Ok(())
    }

    #[test]
    fn exhausted_rollover_preserves_cursor_and_remaining_capacity() -> RS<()> {
        let layout = WorkerLogLayout::new("unused", 1, u64::MAX)?;
        let mut log = ChunkedWorkerLog::new(
            layout,
            WorkerLogTail {
                current_sequence: Some(u64::MAX - 1),
                current_size: u64::MAX - 4,
                next_sequence: u64::MAX,
                next_lsn: 0u64.into(),
            },
        )?;
        assert!(log.reserve_append(5).is_err());
        assert_eq!(log.current_sequence, Some(u64::MAX - 1));
        assert_eq!(log.current_size, u64::MAX - 4);
        assert_eq!(log.next_sequence, u64::MAX);
        assert_eq!(log.reserve_append(4)?.offset, u64::MAX - 4);
        assert!(log.reserve_append(1).is_err());
        assert_eq!(log.current_size, 0);
        assert_eq!(log.current_sequence, None);
        Ok(())
    }
}
