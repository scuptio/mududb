//! `protocol::format::latest` module.
#![allow(missing_docs)]

use super::super::{Frame, FrameHeader, MessageType};
use mudu::common::result::RS;
use mudu::compat::{self, FormatKind, PROTOCOL_FRAME_CURRENT_VERSION};
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_sys_contract::perf::TraceContext;

pub const HEADER_LEN: usize = 40;
pub const MAGIC: u32 = 0x4D53_464D; // "MSFM"
/// Current version of the protocol frame format.
pub const FRAME_VERSION: u32 = PROTOCOL_FRAME_CURRENT_VERSION;

/// Bit 0 of `flags` carries the trace sampling flag.
pub const FLAG_SAMPLED: u64 = 0x0000_0000_0000_0001;

pub fn encode(frame: &Frame) -> Vec<u8> {
    let mut out = Vec::with_capacity(HEADER_LEN + frame.payload.len());
    let trace_context = frame.header.trace_context;
    let flags = frame.header.flags | (trace_context.sampled as u64);
    out.extend_from_slice(&frame.header.magic.to_be_bytes());
    out.extend_from_slice(&frame.header.version.to_be_bytes());
    out.extend_from_slice(&u32::from(frame.header.message_type).to_be_bytes());
    out.extend_from_slice(&flags.to_be_bytes());
    out.extend_from_slice(&frame.header.request_id.to_be_bytes());
    out.extend_from_slice(&trace_context.trace_id.to_be_bytes());
    out.extend_from_slice(&frame.header.payload_len.to_be_bytes());
    out.extend_from_slice(&frame.payload);
    out
}

pub fn decode(buf: &[u8]) -> RS<Frame> {
    if buf.len() < HEADER_LEN {
        return Err(mudu_error!(ErrorCode::Parse, "frame header is incomplete"));
    }
    let header = decode_header_bytes(&buf[..HEADER_LEN])?;
    let payload_len = header.payload_len();
    let total_len = HEADER_LEN + payload_len as usize;
    if buf.len() < total_len {
        return Err(mudu_error!(ErrorCode::Parse, "frame payload is incomplete"));
    }
    Frame::from_parts(header, buf[HEADER_LEN..total_len].to_vec())
}

pub fn decode_header_bytes(buf: &[u8]) -> RS<FrameHeader> {
    if buf.len() < HEADER_LEN {
        return Err(mudu_error!(ErrorCode::Parse, "frame header is incomplete"));
    }
    let magic = u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]);
    let version = u32::from_be_bytes([buf[4], buf[5], buf[6], buf[7]]);
    compat::check_magic_and_version(FormatKind::ProtocolFrame, magic, version)?;
    let message_type =
        MessageType::try_from(u32::from_be_bytes([buf[8], buf[9], buf[10], buf[11]]))?;
    let flags = u64::from_be_bytes([
        buf[12], buf[13], buf[14], buf[15], buf[16], buf[17], buf[18], buf[19],
    ]);
    if flags & !FLAG_SAMPLED != 0 {
        return Err(mudu_error!(
            ErrorCode::Parse,
            "frame header contains unknown flag bits"
        ));
    }
    let request_id = u64::from_be_bytes([
        buf[20], buf[21], buf[22], buf[23], buf[24], buf[25], buf[26], buf[27],
    ]);
    let trace_id = u64::from_be_bytes([
        buf[28], buf[29], buf[30], buf[31], buf[32], buf[33], buf[34], buf[35],
    ]);
    let sampled = (flags & FLAG_SAMPLED) != 0;
    let trace_context = TraceContext { trace_id, sampled };
    let payload_len = u32::from_be_bytes([buf[36], buf[37], buf[38], buf[39]]);
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
