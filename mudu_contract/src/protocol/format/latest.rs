use super::super::{Frame, FrameHeader, MessageType};
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use mudu_sys_contract::perf::TraceContext;

pub const HEADER_LEN: usize = 36;
pub const MAGIC: u32 = 0x4D53_464D; // "MSFM"
pub const FRAME_VERSION: u32 = 1;

pub fn encode(frame: &Frame) -> Vec<u8> {
    let mut out = Vec::with_capacity(HEADER_LEN + frame.payload.len());
    out.extend_from_slice(&frame.header.magic.to_be_bytes());
    out.extend_from_slice(&frame.header.version.to_be_bytes());
    out.extend_from_slice(&u16::from(frame.header.message_type).to_be_bytes());
    out.extend_from_slice(&frame.header.flags.to_be_bytes());
    out.extend_from_slice(&frame.header.request_id.to_be_bytes());
    out.extend_from_slice(&frame.header.trace_context.trace_id.to_be_bytes());
    out.push(frame.header.trace_context.sampled as u8);
    // Reserved padding to keep header 32-byte aligned.
    out.extend_from_slice(&[0u8; 3]);
    out.extend_from_slice(&frame.header.payload_len.to_be_bytes());
    out.extend_from_slice(&frame.payload);
    out
}

pub fn decode(buf: &[u8]) -> RS<Frame> {
    if buf.len() < HEADER_LEN {
        return Err(m_error!(EC::ParseErr, "frame header is incomplete"));
    }
    let header = decode_header_bytes(&buf[..HEADER_LEN])?;
    let payload_len = header.payload_len();
    let total_len = HEADER_LEN + payload_len as usize;
    if buf.len() < total_len {
        return Err(m_error!(EC::ParseErr, "frame payload is incomplete"));
    }
    Frame::from_parts(header, buf[HEADER_LEN..total_len].to_vec())
}

pub fn decode_header_bytes(buf: &[u8]) -> RS<FrameHeader> {
    if buf.len() < HEADER_LEN {
        return Err(m_error!(EC::ParseErr, "frame header is incomplete"));
    }
    let magic = u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]);
    if magic != MAGIC {
        return Err(m_error!(EC::ParseErr, "invalid frame magic"));
    }
    let version = u32::from_be_bytes([buf[4], buf[5], buf[6], buf[7]]);
    if version != FRAME_VERSION {
        return Err(m_error!(
            EC::ParseErr,
            format!("unsupported frame version {}", version)
        ));
    }
    let message_type = MessageType::try_from(u16::from_be_bytes([buf[8], buf[9]]))?;
    let flags = u16::from_be_bytes([buf[10], buf[11]]);
    let request_id = u64::from_be_bytes([
        buf[12], buf[13], buf[14], buf[15], buf[16], buf[17], buf[18], buf[19],
    ]);
    let trace_id = u64::from_be_bytes([
        buf[20], buf[21], buf[22], buf[23], buf[24], buf[25], buf[26], buf[27],
    ]);
    let sampled = buf[28] != 0;
    let trace_context = TraceContext { trace_id, sampled };
    let payload_len = u32::from_be_bytes([buf[32], buf[33], buf[34], buf[35]]);
    Ok(FrameHeader {
        magic,
        version,
        message_type,
        flags,
        request_id,
        trace_context,
        payload_len,
    })
}
