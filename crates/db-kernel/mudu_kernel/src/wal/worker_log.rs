use crate::wal::log_frame::{frame_len, split_frame};
use crate::wal::lsn::LSN;
pub use crate::wal::worker_wal_backend::{
    WalSyncPolicy, WorkerLogBatching, WorkerLogLayout, WorkerLogTail,
    WorkerWALBackend as ChunkedWorkerLogBackend,
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

/// The longest valid frame prefix of a chunk payload, produced by
/// [`scan_valid_frame_prefix`].
pub struct ValidFramePrefix {
    /// Bytes covered by valid frames; equals the payload length when the
    /// whole chunk decoded cleanly.
    pub valid_len: usize,
    /// Decoded frames of the valid prefix, in file order.
    pub frames: Vec<Vec<u8>>,
    /// Highest LSN among the valid frames.
    pub max_lsn: Option<LSN>,
    /// Why the tail was dropped; `None` when the whole payload decoded.
    pub corrupt_reason: Option<String>,
}

/// Scans `payload` as a sequence of log frames and returns the longest
/// valid prefix. The first frame that fails header or CRC validation (bad
/// magic, checksum mismatch, torn/partial frame, zero-filled tail) is
/// treated as end-of-log: a crash can leave an un-fsynced tail behind, and
/// recovery must truncate there instead of failing.
pub fn scan_valid_frame_prefix(payload: &[u8]) -> ValidFramePrefix {
    let mut offset = 0usize;
    let mut frames = Vec::new();
    let mut max_lsn: Option<LSN> = None;
    let mut corrupt_reason = None;
    while offset < payload.len() {
        let remaining = &payload[offset..];
        let next_frame_len = match frame_len(remaining) {
            Ok(len) => len,
            Err(e) => {
                corrupt_reason = Some(format!("invalid frame header at offset {offset}: {e}"));
                break;
            }
        };
        let frame = &remaining[..next_frame_len];
        let lsn = match split_frame(frame) {
            Ok((header, _, _)) => header.lsn(),
            Err(e) => {
                corrupt_reason = Some(format!("invalid frame at offset {offset}: {e}"));
                break;
            }
        };
        max_lsn = Some(max_lsn.map_or(lsn, |current| current.max(lsn)));
        frames.push(frame.to_vec());
        offset += next_frame_len;
    }
    ValidFramePrefix {
        valid_len: offset,
        frames,
        max_lsn,
        corrupt_reason,
    }
}

/// Decode complete log frames and silently drop a corrupt trailing tail.
///
/// WAL chunks may end in an incomplete or un-fsynced final frame if the
/// writer (or the machine) was interrupted before the tail was fully
/// persisted. Recovery ignores such trailing bytes rather than failing;
/// see [`scan_valid_frame_prefix`] for exactly what is tolerated.
pub fn decode_frames_allow_trailing(payload: &[u8]) -> RS<Vec<Vec<u8>>> {
    Ok(scan_valid_frame_prefix(payload).frames)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use crate::wal::log_frame::serialize_entry;
    use std::sync::atomic::AtomicU64;

    fn valid_frames_payload(count: u64) -> (Vec<u8>, usize) {
        let next_lsn = AtomicU64::new(0);
        let mut payload = Vec::new();
        let mut frames = 0usize;
        for i in 0..count {
            let parts = serialize_entry(&i, 4096, &next_lsn).unwrap();
            frames += parts.len();
            payload.extend(parts.into_iter().flatten());
        }
        (payload, frames)
    }

    #[test]
    fn scan_valid_frame_prefix_clean_payload_decodes_everything() {
        let (payload, frames) = valid_frames_payload(3);
        let prefix = scan_valid_frame_prefix(&payload);
        assert_eq!(prefix.valid_len, payload.len());
        assert_eq!(prefix.frames.len(), frames);
        assert_eq!(prefix.max_lsn, Some(LSN::new(2)));
        assert!(prefix.corrupt_reason.is_none());
    }

    #[test]
    fn scan_valid_frame_prefix_empty_payload_is_clean() {
        let prefix = scan_valid_frame_prefix(&[]);
        assert_eq!(prefix.valid_len, 0);
        assert!(prefix.frames.is_empty());
        assert_eq!(prefix.max_lsn, None);
        assert!(prefix.corrupt_reason.is_none());
    }

    #[test]
    fn scan_valid_frame_prefix_drops_partial_frame_tail() {
        let (mut payload, frames) = valid_frames_payload(2);
        let valid_len = payload.len();
        // A torn final frame of at least header+tailer size: the header is
        // intact but the payload never made it to disk.
        let next_lsn = AtomicU64::new(2);
        let tail = serialize_entry(&vec![7u8; 128], 4096, &next_lsn).unwrap();
        payload.extend_from_slice(&tail[0][..40]);
        let prefix = scan_valid_frame_prefix(&payload);
        assert_eq!(prefix.valid_len, valid_len);
        assert_eq!(prefix.frames.len(), frames);
        assert_eq!(prefix.max_lsn, Some(LSN::new(1)));
        assert!(prefix.corrupt_reason.is_some());
    }

    #[test]
    fn scan_valid_frame_prefix_drops_zero_filled_tail() {
        let (mut payload, frames) = valid_frames_payload(2);
        let valid_len = payload.len();
        payload.extend_from_slice(&[0u8; 128]);
        let prefix = scan_valid_frame_prefix(&payload);
        assert_eq!(prefix.valid_len, valid_len);
        assert_eq!(prefix.frames.len(), frames);
        assert_eq!(prefix.max_lsn, Some(LSN::new(1)));
        assert!(prefix.corrupt_reason.is_some());
    }

    #[test]
    fn scan_valid_frame_prefix_drops_checksum_corrupted_tail() {
        let (mut payload, frames) = valid_frames_payload(2);
        let valid_len = payload.len();
        let (mut tail, _) = valid_frames_payload(1);
        // Corrupt one payload byte of the tail frame so its CRC fails.
        let index = tail.len() - 9;
        tail[index] ^= 0x7f;
        payload.extend_from_slice(&tail);
        let prefix = scan_valid_frame_prefix(&payload);
        assert_eq!(prefix.valid_len, valid_len);
        assert_eq!(prefix.frames.len(), frames);
        assert_eq!(prefix.max_lsn, Some(LSN::new(1)));
        assert!(prefix.corrupt_reason.is_some());
    }

    #[test]
    fn decode_frames_allow_trailing_tolerates_corrupt_tail() {
        let (mut payload, frames) = valid_frames_payload(1);
        payload.extend_from_slice(&[0u8; 64]);
        let decoded = decode_frames_allow_trailing(&payload).unwrap();
        assert_eq!(decoded.len(), frames);
    }
}
