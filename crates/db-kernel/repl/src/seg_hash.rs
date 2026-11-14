//! Segment hash chain for raft-committed WAL segments.
//!
//! Every worker's WAL stream is cut into segments at the group-commit
//! watermark boundaries. A segment's identity on every node is
//! `seg_hash = H(prev_hash ‖ seg_bytes)`: a chain over the segment sequence,
//! anchored at the zero hash on the chain genesis. The committed vector (the
//! last `CommitVec` per worker) is the trust anchor; a node recomputing the
//! chain over its own frames detects divergence at the first mismatching
//! segment.

/// 32-byte segment hash carried by `ReplEntry::CommitVec`.
pub type SegHash = [u8; 32];

/// Chain genesis: the `prev_hash` of the first segment of every worker.
pub const ZERO_SEG_HASH: SegHash = [0u8; 32];

/// Extends the chain over one segment's frame bytes.
///
/// Derivation (documented, deterministic): the 32-byte hash is four CRC64 passes with domain
/// separation — for `i` in `0..4`, `h_i = crc64(i_be ‖ prev_hash ‖ seg_bytes)`,
/// concatenated big-endian. `seg_bytes` is the length-framed frame sequence:
/// for every frame in the segment, in LSN order, `len_be_u64 ‖ frame_bytes`.
/// Incremental updates preserve the exact byte sequence and checksum values
/// without allocating concatenated segment or per-domain input buffers.
pub(crate) fn seg_hash_chain(prev: &SegHash, frames: &[Vec<u8>]) -> SegHash {
    let mut out = [0u8; 32];
    for (i, chunk) in out.chunks_mut(8).enumerate() {
        let mut digest = mudu::common::crc::Crc64::new();
        digest.update(&(i as u64).to_be_bytes());
        digest.update(prev);
        for frame in frames {
            digest.update(&(frame.len() as u64).to_be_bytes());
            digest.update(frame);
        }
        chunk.copy_from_slice(&digest.finalize().to_be_bytes());
    }
    out
}

/// Renders a segment hash as lowercase hex (topology endpoint, logs).
pub fn seg_hash_hex(hash: &SegHash) -> String {
    let mut out = String::with_capacity(64);
    for byte in hash {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;

    #[test]
    fn chain_is_deterministic_and_order_sensitive() {
        let frames = vec![b"aaa".to_vec(), b"bbbb".to_vec()];
        let h1 = seg_hash_chain(&ZERO_SEG_HASH, &frames);
        let h2 = seg_hash_chain(&ZERO_SEG_HASH, &frames);
        assert_eq!(h1, h2);
        let swapped = vec![b"bbbb".to_vec(), b"aaa".to_vec()];
        assert_ne!(h1, seg_hash_chain(&ZERO_SEG_HASH, &swapped));
        let next = seg_hash_chain(&h1, &frames);
        assert_ne!(next, h1);
        assert_ne!(next, seg_hash_chain(&ZERO_SEG_HASH, &frames));
    }

    #[test]
    fn hex_renders_64_chars() {
        let h = seg_hash_chain(&ZERO_SEG_HASH, &[vec![7u8; 3]]);
        assert_eq!(seg_hash_hex(&h).len(), 64);
    }
    /// Frozen pre-streaming derivation, retained as a byte-compatibility oracle.
    fn concatenated_reference(prev: &SegHash, frames: &[Vec<u8>]) -> SegHash {
        let mut out = [0; 32];
        for (i, chunk) in out.chunks_mut(8).enumerate() {
            let mut bytes = (i as u64).to_be_bytes().to_vec();
            bytes.extend_from_slice(prev);
            for frame in frames {
                bytes.extend_from_slice(&(frame.len() as u64).to_be_bytes());
                bytes.extend_from_slice(frame);
            }
            chunk.copy_from_slice(&mudu::common::crc::crc64(&bytes).to_be_bytes());
        }
        out
    }

    #[test]
    fn streaming_chain_matches_old_encoding_and_frame_boundaries() {
        let cases = [
            vec![],
            vec![vec![]],
            vec![vec![], b"abc".to_vec(), vec![]],
            vec![b"a".to_vec(), b"bc".to_vec()],
            vec![b"ab".to_vec(), b"c".to_vec()],
            vec![vec![0x5a; 16384], vec![0xa5; 65535]],
        ];
        for prev in [ZERO_SEG_HASH, [0xa7; 32]] {
            for frames in &cases {
                let actual = seg_hash_chain(&prev, frames);
                assert_eq!(actual, concatenated_reference(&prev, frames));
                assert_eq!(
                    seg_hash_chain(&actual, frames),
                    concatenated_reference(&actual, frames)
                );
            }
            assert_ne!(
                seg_hash_chain(&prev, &cases[0]),
                seg_hash_chain(&prev, &cases[1])
            );
            assert_ne!(
                seg_hash_chain(&prev, &cases[3]),
                seg_hash_chain(&prev, &cases[4])
            );
        }
    }
}
