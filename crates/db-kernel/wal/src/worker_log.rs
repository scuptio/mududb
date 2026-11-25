use crate::log_frame::{frame_len, split_frame};
use crate::lsn::LSN;
pub use crate::worker_wal_backend::{
    ArchiveFrontier, GcFrontier, WalSyncPolicy, WorkerLogBatching, WorkerLogLayout, WorkerLogTail,
    WorkerWALBackend as ChunkedWorkerLogBackend,
};
use async_trait::async_trait;
use mudu::common::result::RS;
use serde::Serialize;
use std::path::{Path, PathBuf};

/// Storage backend of a worker write-ahead log.
///
/// Serializes entries into log frames, appends the frames to the log's chunk
/// files, and flushes them durable.
#[async_trait]
pub trait WorkerLogBackend: Clone + Send + Sync {
    /// Maximum byte size of one encoded frame (payload plus header and
    /// tailer) this backend accepts.
    fn frame_size_limit(&self) -> RS<usize>;

    /// Serializes an entry into one or more frames, allocating their LSNs.
    fn serialize_entry<L: Serialize + Send + Sync>(&self, entry: &L) -> RS<Vec<Vec<u8>>>;
    /// Paths of the log's chunk files in chunk-sequence order.
    async fn chunk_paths_sorted(&self) -> RS<Vec<PathBuf>>;
    /// Appends already-framed bytes to the log.
    async fn append_frames_async(&self, frames: Vec<Vec<u8>>) -> RS<()>;

    /// Flushes the current chunk to stable storage (blocking).
    fn flush(&self) -> RS<()>;
    /// Flushes the current chunk to stable storage.
    async fn flush_async(&self) -> RS<()>;
}

/// Read access to persisted log chunks during recovery.
#[async_trait]
pub trait WorkerLogRecoverySource {
    /// Paths of the log's chunk files in chunk-sequence order.
    async fn chunk_paths_sorted(&self) -> RS<Vec<PathBuf>>;
    /// Reads one chunk file's full content.
    async fn read_chunk(&self, path: &Path) -> RS<Vec<u8>>;
}

/// Read access to persisted log chunks during recovery, for sources that
/// need mutable access to themselves.
#[async_trait]
pub trait AsyncWorkerLogRecoverySource: Send {
    /// Paths of the log's chunk files in chunk-sequence order.
    async fn chunk_paths_sorted(&mut self) -> RS<Vec<PathBuf>>;
    /// Reads one chunk file's full content.
    async fn read_chunk(&mut self, path: &Path) -> RS<Vec<u8>>;
}

/// Splits a chunk payload into its frames, failing on the first invalid
/// frame.
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
    let mut prefix = crate::frame_bounds::Prefix {
        offset: 0,
        stopped: false,
    };
    let mut frames = Vec::new();
    let mut max_lsn: Option<LSN> = None;
    let mut corrupt_reason = None;
    while prefix.offset < payload.len() && !prefix.stopped {
        let offset = prefix.offset;
        let remaining = &payload[offset..];
        let next_frame_len = match frame_len(remaining) {
            Ok(len) => len,
            Err(e) => {
                corrupt_reason = Some(format!("invalid frame header at offset {offset}: {e}"));
                prefix = prefix.advance(payload.len(), 0, false);
                break;
            }
        };
        let next = prefix.advance(payload.len(), next_frame_len, true);
        if next.stopped {
            prefix = next;
            corrupt_reason = Some(format!("invalid frame bounds at offset {offset}"));
            break;
        }
        let frame = &payload[offset..next.offset];
        let lsn = match split_frame(frame) {
            Ok((header, _, _)) => header.lsn(),
            Err(e) => {
                corrupt_reason = Some(format!("invalid frame at offset {offset}: {e}"));
                prefix = prefix.advance(payload.len(), next_frame_len, false);
                break;
            }
        };
        max_lsn = Some(max_lsn.map_or(lsn, |current| current.max(lsn)));
        frames.push(frame.to_vec());
        prefix = next;
    }
    ValidFramePrefix {
        valid_len: prefix.offset,
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
    use crate::log_frame::{serialize_entry, LOG_FRAME_HEADER_SIZE};
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

    /// Serializes `count` single-frame entries and returns the frames
    /// individually so tests can reason about frame boundaries.
    fn single_frames(count: u64) -> Vec<Vec<u8>> {
        let next_lsn = AtomicU64::new(0);
        let mut frames = Vec::new();
        for i in 0..count {
            let parts = serialize_entry(&i, 4096, &next_lsn).unwrap();
            assert_eq!(parts.len(), 1, "test entries must fit in one frame");
            frames.extend(parts);
        }
        frames
    }

    fn flatten_frames(frames: &[Vec<u8>]) -> Vec<u8> {
        frames.iter().flatten().copied().collect()
    }

    /// Deterministic non-zero garbage that never starts with the frame magic.
    fn garbage_bytes(len: usize) -> Vec<u8> {
        (0..len).map(|i| (i * 37 + 11) as u8).collect()
    }

    #[test]
    fn every_byte_truncation_preserves_exact_complete_frame_prefix() {
        let frames = single_frames(3);
        let payload = flatten_frames(&frames);
        for end in 0..=payload.len() {
            let prefix = scan_valid_frame_prefix(&payload[..end]);
            let mut expected_len = 0;
            let mut expected_count = 0;
            for frame in &frames {
                if expected_len + frame.len() > end {
                    break;
                }
                expected_len += frame.len();
                expected_count += 1;
            }
            assert_eq!(prefix.valid_len, expected_len, "cut at {end}");
            assert_eq!(prefix.frames, frames[..expected_count], "cut at {end}");
            assert_eq!(prefix.corrupt_reason.is_some(), expected_len != end);
        }
    }

    #[test]
    fn oversized_declared_length_and_extra_frame_bytes_are_rejected() {
        let frames = single_frames(1);
        let mut frame = frames[0].clone();
        frame[16..20].copy_from_slice(&u32::MAX.to_be_bytes());
        assert!(frame_len(&frame).is_err());
        assert!(split_frame(&frame).is_err());
        assert_eq!(scan_valid_frame_prefix(&frame).valid_len, 0);
        let mut extra = frames[0].clone();
        extra.push(0);
        assert!(split_frame(&extra).is_err());
        assert_eq!(scan_valid_frame_prefix(&extra).valid_len, frames[0].len());
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

    #[test]
    fn scan_valid_frame_prefix_stops_at_corrupted_middle_frame() {
        let frames = single_frames(3);
        let first_frame_len = frames[0].len();
        let mut payload = flatten_frames(&frames);
        // Corrupt one payload byte of the middle frame: frames before it stay
        // valid, the valid frame after it is dropped with the corrupt one.
        payload[first_frame_len + LOG_FRAME_HEADER_SIZE] ^= 0x7f;
        let prefix = scan_valid_frame_prefix(&payload);
        assert_eq!(prefix.valid_len, first_frame_len);
        assert_eq!(prefix.frames, vec![frames[0].clone()]);
        assert_eq!(prefix.max_lsn, Some(LSN::new(0)));
        let reason = prefix.corrupt_reason.unwrap();
        assert!(
            reason.contains(&format!("offset {first_frame_len}")),
            "unexpected corrupt reason: {reason}"
        );
    }

    #[test]
    fn scan_valid_frame_prefix_stops_at_middle_frame_with_bad_magic() {
        let frames = single_frames(3);
        let first_frame_len = frames[0].len();
        let mut payload = flatten_frames(&frames);
        // The first byte of the middle frame is part of its magic.
        payload[first_frame_len] ^= 0x7f;
        let prefix = scan_valid_frame_prefix(&payload);
        assert_eq!(prefix.valid_len, first_frame_len);
        assert_eq!(prefix.frames, vec![frames[0].clone()]);
        assert_eq!(prefix.max_lsn, Some(LSN::new(0)));
        assert!(prefix.corrupt_reason.is_some());
    }

    #[test]
    fn scan_valid_frame_prefix_drops_garbage_tail() {
        let (mut payload, frames) = valid_frames_payload(2);
        let valid_len = payload.len();
        // Non-zero garbage: fails the magic check, unlike the zero-filled
        // tail case.
        payload.extend_from_slice(&garbage_bytes(200));
        let prefix = scan_valid_frame_prefix(&payload);
        assert_eq!(prefix.valid_len, valid_len);
        assert_eq!(prefix.frames.len(), frames);
        assert_eq!(prefix.max_lsn, Some(LSN::new(1)));
        assert!(prefix.corrupt_reason.is_some());
    }

    #[test]
    fn scan_valid_frame_prefix_drops_tail_with_valid_magic_but_bad_version() {
        let (mut payload, frames) = valid_frames_payload(2);
        let valid_len = payload.len();
        // A full frame whose magic and CRC are intact but whose format
        // version is unsupported (current is 1): header validation must
        // still reject it.
        let (mut tail, _) = valid_frames_payload(1);
        tail[4..8].copy_from_slice(&2u32.to_be_bytes());
        payload.extend_from_slice(&tail);
        let prefix = scan_valid_frame_prefix(&payload);
        assert_eq!(prefix.valid_len, valid_len);
        assert_eq!(prefix.frames.len(), frames);
        assert_eq!(prefix.max_lsn, Some(LSN::new(1)));
        assert!(prefix.corrupt_reason.is_some());
    }

    #[test]
    fn scan_valid_frame_prefix_garbage_only_payload_yields_empty_prefix() {
        let prefix = scan_valid_frame_prefix(&garbage_bytes(256));
        assert_eq!(prefix.valid_len, 0);
        assert!(prefix.frames.is_empty());
        assert_eq!(prefix.max_lsn, None);
        assert!(prefix.corrupt_reason.is_some());
    }

    #[test]
    fn scan_valid_frame_prefix_drops_tails_shorter_than_one_frame() {
        let (payload, frames) = valid_frames_payload(2);
        let valid_len = payload.len();
        let (tail_source, _) = valid_frames_payload(1);
        // Tails that cannot even hold header + tailer, plus a tail cut one
        // byte short of a full frame: every truncation depth must drop the
        // tail without panicking.
        for cut in [
            1,
            LOG_FRAME_HEADER_SIZE - 1,
            LOG_FRAME_HEADER_SIZE,
            LOG_FRAME_HEADER_SIZE + 1,
            tail_source.len() - 1,
        ] {
            let mut truncated = payload.clone();
            truncated.extend_from_slice(&tail_source[..cut]);
            let prefix = scan_valid_frame_prefix(&truncated);
            assert_eq!(prefix.valid_len, valid_len, "cut length {cut}");
            assert_eq!(prefix.frames.len(), frames, "cut length {cut}");
            assert_eq!(prefix.max_lsn, Some(LSN::new(1)), "cut length {cut}");
            assert!(prefix.corrupt_reason.is_some(), "cut length {cut}");
        }
    }

    #[test]
    fn decode_frames_rejects_corrupt_payloads_that_allow_trailing_tolerates() {
        // Garbage after a valid frame: strict decoding fails, recovery
        // decoding keeps the valid frame.
        let (mut payload, frames) = valid_frames_payload(1);
        payload.extend_from_slice(&garbage_bytes(64));
        assert!(decode_frames(&payload).is_err());
        assert_eq!(
            decode_frames_allow_trailing(&payload).unwrap().len(),
            frames
        );

        // A checksum-corrupted frame: same split between the two decoders.
        let (mut corrupted, _) = valid_frames_payload(1);
        let index = corrupted.len() - 9;
        corrupted[index] ^= 0x7f;
        assert!(decode_frames(&corrupted).is_err());
        assert!(decode_frames_allow_trailing(&corrupted).unwrap().is_empty());
    }
}
