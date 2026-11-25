//! The commit-segment wire format of a worker's data stream.
//!
//! One segment is the contiguous run of WAL frames produced between two
//! group-commit watermarks — exactly the cut the old commit sequencer made.
//! The segment travels as one stream record (its frames are contiguous and
//! continue the machine's committed WAL point).

use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use serde::{Deserialize, Serialize};
use wal::log_frame::frame_lsn;

/// One commit segment: the WAL frames of `[lo..=hi]` in LSN order.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DataSegment {
    /// Format version (currently one).
    pub version: u32,
    /// The segment's frames, contiguous and in LSN order.
    pub frames: Vec<Vec<u8>>,
}

impl DataSegment {
    /// Encodes a segment, enforcing the one-frame record budget: a segment
    /// must fit a single stream frame (the batching watermarks keep it so).
    pub fn encode(&self) -> RS<Vec<u8>> {
        let bytes = rmp_serde::to_vec(self)
            .map_err(|e| mudu_error!(ErrorCode::Encode, "encode data segment", e))?;
        Ok(bytes)
    }

    /// Decodes a segment record payload.
    pub fn decode(bytes: &[u8]) -> RS<Self> {
        rmp_serde::from_slice(bytes)
            .map_err(|e| mudu_error!(ErrorCode::Decode, "decode data segment", e))
    }

    /// Validates the segment is non-empty and contiguous, returning its
    /// `(first_lsn, last_lsn)` WAL range.
    pub fn lsn_range(&self) -> RS<(u64, u64)> {
        let mut first = None;
        let mut prev = None;
        for frame in &self.frames {
            let lsn = frame_lsn(frame)?.as_u64();
            if let Some(prev) = prev {
                if lsn != prev + 1 {
                    return Err(mudu_error!(
                        ErrorCode::InvalidState,
                        format!("data segment frame gap: {prev} then {lsn}")
                    ));
                }
            } else {
                first = Some(lsn);
            }
            prev = Some(lsn);
        }
        match (first, prev) {
            (Some(lo), Some(hi)) => Ok((lo, hi)),
            _ => Err(mudu_error!(ErrorCode::InvalidState, "empty data segment")),
        }
    }
}
