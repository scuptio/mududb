//! Frame-decoding helpers shared by the server and replication layers.

use mudu::common::result::RS;
use mudu_contract::protocol::{Frame, FrameHeader, HEADER_LEN};

/// Attempt to decode one frame from the front of `buf`.
///
/// Returns `Ok(None)` when the buffer does not yet hold a complete frame
/// header or payload; otherwise returns the decoded frame and the number of
/// bytes it consumed.
pub fn try_decode_next_frame(buf: &[u8]) -> RS<Option<(Frame, usize)>> {
    if buf.len() < HEADER_LEN {
        return Ok(None);
    }
    let header = FrameHeader::decode_header_bytes(&buf[..HEADER_LEN])?;
    let payload_len = header.payload_len() as usize;
    let frame_len = HEADER_LEN + payload_len;
    if buf.len() < frame_len {
        return Ok(None);
    }
    let frame = Frame::decode(&buf[..frame_len])?;
    Ok(Some((frame, frame_len)))
}
