//! Local implementation of the MuduDB log frame format v1.
//!
//! Byte-compatible with `doc/en/contract/log_frame_v1.md` and
//! `mudu_kernel/src/wal/format/latest.rs` (which this crate must not depend
//! on):
//!
//! - 24-byte big-endian header: magic `LGFM` (`0x4C47_464D`), version,
//!   8-byte `lsn`, payload `size`, `n_part` (remaining frames of the same
//!   logical record);
//! - variable payload;
//! - 8-byte big-endian tailer: `n_part` (must match the header) and
//!   `checksum`, the lower 32 bits of the CRC64-XZ of the payload (via
//!   [`mudu::common::crc::calc_crc`], exactly what the WAL uses).
//!
//! A logical record larger than the part limit is split into multiple
//! frames whose `n_part` counts down to 0; payloads are concatenated to
//! reassemble the record.

use mudu::common::crc::calc_crc;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;

/// Frame magic: ASCII `LGFM`.
pub const LOG_FRAME_MAGIC: u32 = 0x4C47_464D;
/// Only supported frame version.
pub const LOG_FRAME_VERSION: u32 = 1;
/// Header size in bytes.
pub const HEADER_SIZE: usize = 24;
/// Tailer size in bytes.
pub const TAILER_SIZE: usize = 8;

/// One validated frame extracted from a chunk buffer.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ScannedFrame {
    /// The `lsn` field of the frame header (raft log index for entry
    /// records, 0 for other record kinds).
    pub lsn: u64,
    /// Number of remaining frames of the same record after this one.
    pub n_part: u32,
    /// Frame payload.
    pub payload: Vec<u8>,
}

/// Result of scanning a chunk buffer: the longest valid frame prefix.
pub struct ScannedPrefix {
    /// Number of bytes covered by valid frames (the torn tail starts here).
    pub valid_len: usize,
    /// Valid frames in order.
    pub frames: Vec<ScannedFrame>,
    /// Why the scan stopped, if it stopped before the end of the buffer.
    pub corrupt_reason: Option<String>,
}

fn payload_checksum(payload: &[u8]) -> u32 {
    calc_crc(payload) as u32
}

/// Serializes one record payload into one or more concatenated frames.
///
/// `lsn` is stamped into every frame header of the record; `max_part_size`
/// bounds the size of a single frame (header + payload + tailer).
pub fn serialize_record(payload: &[u8], lsn: u64, max_part_size: usize) -> RS<Vec<u8>> {
    if max_part_size <= HEADER_SIZE + TAILER_SIZE {
        return Err(mudu_error!(
            ErrorCode::Parse,
            "max_part_size must be larger than header + tailer"
        ));
    }
    let max_payload_size = max_part_size - HEADER_SIZE - TAILER_SIZE;
    let total_parts = payload.len().div_ceil(max_payload_size).max(1);
    let mut out = Vec::with_capacity(payload.len() + total_parts * (HEADER_SIZE + TAILER_SIZE));
    for (index, chunk) in payload.chunks(max_payload_size).enumerate() {
        let remaining = (total_parts - index - 1) as u32;
        out.extend_from_slice(&LOG_FRAME_MAGIC.to_be_bytes());
        out.extend_from_slice(&LOG_FRAME_VERSION.to_be_bytes());
        out.extend_from_slice(&lsn.to_be_bytes());
        out.extend_from_slice(&(chunk.len() as u32).to_be_bytes());
        out.extend_from_slice(&remaining.to_be_bytes());
        out.extend_from_slice(chunk);
        out.extend_from_slice(&remaining.to_be_bytes());
        out.extend_from_slice(&payload_checksum(chunk).to_be_bytes());
    }
    Ok(out)
}

/// Returns the total byte length of the frame at the start of `input`.
pub fn frame_len(input: &[u8]) -> RS<usize> {
    if input.len() < HEADER_SIZE + TAILER_SIZE {
        return Err(mudu_error!(ErrorCode::Decode, "log frame is truncated"));
    }
    let magic = u32::from_be_bytes(input[0..4].try_into().unwrap_or([0; 4]));
    if magic != LOG_FRAME_MAGIC {
        return Err(mudu_error!(
            ErrorCode::Decode,
            format!("invalid log frame magic {magic:#x}")
        ));
    }
    let version = u32::from_be_bytes(input[4..8].try_into().unwrap_or([0; 4]));
    if version != LOG_FRAME_VERSION {
        return Err(mudu_error!(
            ErrorCode::Decode,
            format!("unsupported log frame version {version}")
        ));
    }
    let size = u32::from_be_bytes(input[16..20].try_into().unwrap_or([0; 4])) as usize;
    let expected_len = HEADER_SIZE + size + TAILER_SIZE;
    if input.len() < expected_len {
        return Err(mudu_error!(
            ErrorCode::Decode,
            format!("log frame is truncated, expected at least {expected_len}")
        ));
    }
    Ok(expected_len)
}

/// Validates and splits one frame (which must be exactly `frame_len` long).
pub fn split_frame(frame: &[u8]) -> RS<ScannedFrame> {
    let expected_len = frame_len(frame)?;
    if frame.len() != expected_len {
        return Err(mudu_error!(
            ErrorCode::Decode,
            format!(
                "log frame length mismatch, expected {}, got {}",
                expected_len,
                frame.len()
            )
        ));
    }
    let lsn = u64::from_be_bytes(frame[8..16].try_into().unwrap_or([0; 8]));
    let size = u32::from_be_bytes(frame[16..20].try_into().unwrap_or([0; 4])) as usize;
    let n_part = u32::from_be_bytes(frame[20..24].try_into().unwrap_or([0; 4]));
    let payload = &frame[HEADER_SIZE..HEADER_SIZE + size];
    let tailer = &frame[HEADER_SIZE + size..];
    let tailer_n_part = u32::from_be_bytes(tailer[0..4].try_into().unwrap_or([0; 4]));
    let tailer_checksum = u32::from_be_bytes(tailer[4..8].try_into().unwrap_or([0; 4]));
    if n_part != tailer_n_part {
        return Err(mudu_error!(
            ErrorCode::Decode,
            "log frame header/tailer n_part mismatch"
        ));
    }
    if payload_checksum(payload) != tailer_checksum {
        return Err(mudu_error!(
            ErrorCode::Decode,
            "log frame payload checksum mismatch"
        ));
    }
    Ok(ScannedFrame {
        lsn,
        n_part,
        payload: payload.to_vec(),
    })
}

/// Scans a chunk buffer and returns its longest valid frame prefix.
///
/// Mirrors `mudu_kernel::wal::worker_log::scan_valid_frame_prefix`: the scan
/// stops at the first invalid frame; trailing bytes after that point are
/// the torn tail left by an interrupted write.
pub fn scan_valid_frame_prefix(payload: &[u8]) -> ScannedPrefix {
    let mut offset = 0usize;
    let mut frames = Vec::new();
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
        match split_frame(frame) {
            Ok(scanned) => frames.push(scanned),
            Err(e) => {
                corrupt_reason = Some(format!("invalid frame at offset {offset}: {e}"));
                break;
            }
        }
        offset += next_frame_len;
    }
    ScannedPrefix {
        valid_len: offset,
        frames,
        corrupt_reason,
    }
}

/// Reassembles scanned frames into record payloads `(lsn, payload)`.
///
/// Frames of a multi-part record must be consecutive with `n_part`
/// counting down to 0. Incomplete trailing records (a torn tail that lost
/// the final parts) are dropped silently; a malformed countdown in the
/// middle is an error.
pub fn reassemble_records(frames: &[ScannedFrame]) -> RS<Vec<(u64, Vec<u8>)>> {
    let mut records = Vec::new();
    let mut pending: Vec<&ScannedFrame> = Vec::new();
    for frame in frames {
        if pending.is_empty() {
            pending.push(frame);
        } else {
            let expected = pending.len();
            let first_remaining = pending[0].n_part as usize;
            let expected_remaining = first_remaining - expected;
            if frame.n_part as usize != expected_remaining {
                return Err(mudu_error!(
                    ErrorCode::Decode,
                    format!(
                        "unexpected log frame order, expected remaining {}, got {}",
                        expected_remaining, frame.n_part
                    )
                ));
            }
            pending.push(frame);
        }
        if frame.n_part == 0 {
            let mut payload = Vec::new();
            for part in &pending {
                payload.extend_from_slice(&part.payload);
            }
            records.push((pending[0].lsn, payload));
            pending.clear();
        }
    }
    // leftover pending frames are an incomplete trailing record: dropped
    Ok(records)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_frame_roundtrip() {
        let payload = b"hello raft".to_vec();
        let buf = serialize_record(&payload, 42, 1024).unwrap();
        assert_eq!(buf.len(), HEADER_SIZE + payload.len() + TAILER_SIZE);
        let scanned = scan_valid_frame_prefix(&buf);
        assert_eq!(scanned.valid_len, buf.len());
        assert!(scanned.corrupt_reason.is_none());
        assert_eq!(scanned.frames.len(), 1);
        assert_eq!(scanned.frames[0].lsn, 42);
        assert_eq!(scanned.frames[0].n_part, 0);
        let records = reassemble_records(&scanned.frames).unwrap();
        assert_eq!(records, vec![(42, payload)]);
    }

    #[test]
    fn multi_part_record_roundtrip() {
        let payload: Vec<u8> = (0..10_000u32).map(|i| (i % 251) as u8).collect();
        let buf = serialize_record(&payload, 7, 1024).unwrap();
        let scanned = scan_valid_frame_prefix(&buf);
        assert_eq!(scanned.valid_len, buf.len());
        assert!(scanned.frames.len() > 1);
        // n_part counts down to 0
        let n = scanned.frames.len();
        for (i, f) in scanned.frames.iter().enumerate() {
            assert_eq!(f.n_part as usize, n - i - 1);
            assert_eq!(f.lsn, 7);
        }
        let records = reassemble_records(&scanned.frames).unwrap();
        assert_eq!(records, vec![(7, payload)]);
    }

    #[test]
    fn torn_tail_is_dropped() {
        let buf1 = serialize_record(b"first", 1, 1024).unwrap();
        let buf2 = serialize_record(b"second", 2, 1024).unwrap();
        let mut buf = buf1.clone();
        buf.extend_from_slice(&buf2);
        // tear the tail: chop the last frame in half
        let torn_len = buf.len() - 7;
        buf.truncate(torn_len);
        let scanned = scan_valid_frame_prefix(&buf);
        assert_eq!(scanned.valid_len, buf1.len());
        assert!(scanned.corrupt_reason.is_some());
        let records = reassemble_records(&scanned.frames).unwrap();
        assert_eq!(records, vec![(1, b"first".to_vec())]);
    }

    #[test]
    fn corrupt_checksum_is_dropped() {
        let mut buf = serialize_record(b"data", 9, 1024).unwrap();
        // flip a payload byte
        buf[HEADER_SIZE] ^= 0xff;
        let scanned = scan_valid_frame_prefix(&buf);
        assert_eq!(scanned.valid_len, 0);
        assert!(scanned.corrupt_reason.is_some());
    }

    #[test]
    fn bad_magic_is_rejected() {
        let mut buf = serialize_record(b"data", 9, 1024).unwrap();
        buf[0] = 0;
        assert!(frame_len(&buf).is_err());
    }

    #[test]
    fn incomplete_multipart_record_is_dropped() {
        let payload = vec![0u8; 5000];
        let buf = serialize_record(&payload, 3, 1024).unwrap();
        let scanned = scan_valid_frame_prefix(&buf);
        assert!(scanned.frames.len() > 1);
        // drop the final part
        let frames = &scanned.frames[..scanned.frames.len() - 1];
        let records = reassemble_records(frames).unwrap();
        assert!(records.is_empty());
    }

    #[test]
    fn header_layout_is_lgfm_v1() {
        // Golden-bytes test pinning the exact header layout of
        // doc/en/contract/log_frame_v1.md.
        let buf = serialize_record(b"ab", 0x0102_0304_0506_0708, 1024).unwrap();
        assert_eq!(&buf[0..4], b"LGFM");
        assert_eq!(&buf[4..8], &1u32.to_be_bytes());
        assert_eq!(&buf[8..16], &0x0102_0304_0506_0708u64.to_be_bytes());
        assert_eq!(&buf[16..20], &2u32.to_be_bytes());
        assert_eq!(&buf[20..24], &0u32.to_be_bytes());
        assert_eq!(&buf[24..26], b"ab");
        let tailer = &buf[26..34];
        assert_eq!(&tailer[0..4], &0u32.to_be_bytes());
        let crc = (calc_crc(b"ab") as u32).to_be_bytes();
        assert_eq!(&tailer[4..8], &crc);
        assert_eq!(buf.len(), 34);
    }
}
