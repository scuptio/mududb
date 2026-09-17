//! `protocol::format::mod` module.
#![allow(missing_docs)]

pub mod latest;

use super::{Frame, FrameHeader};
use mudu::common::result::RS;
use mudu::compat::{self, FormatKind, PROTOCOL_FRAME_CURRENT_VERSION};
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_compat_migrate::{NoopOptionProvider, global};
use std::borrow::Cow;

pub use latest::HEADER_LEN;

pub fn encode_latest(frame: &Frame) -> Vec<u8> {
    latest::encode(frame)
}

fn ensure_latest_protocol_frame(buf: &[u8], version: u32) -> RS<Cow<'_, [u8]>> {
    if version == PROTOCOL_FRAME_CURRENT_VERSION {
        return Ok(Cow::Borrowed(buf));
    }
    global::upgrade_to_current(FormatKind::ProtocolFrame, version, buf, &NoopOptionProvider)
        .map(Cow::Owned)
        .map_err(|e| e.into_mudu_error())
}

pub fn decode(buf: &[u8]) -> RS<Frame> {
    let (magic, version) = peek_magic_and_version(buf)?;
    compat::check_magic_and_version(FormatKind::ProtocolFrame, magic, version)?;
    let migrated = ensure_latest_protocol_frame(buf, version)?;
    latest::decode(&migrated)
}

pub fn decode_header_bytes(buf: &[u8]) -> RS<FrameHeader> {
    let (magic, version) = peek_magic_and_version(buf)?;
    compat::check_magic_and_version(FormatKind::ProtocolFrame, magic, version)?;
    let migrated = ensure_latest_protocol_frame(buf, version)?;
    latest::decode_header_bytes(&migrated)
}

fn peek_magic_and_version(buf: &[u8]) -> RS<(u32, u32)> {
    if buf.len() < HEADER_LEN {
        return Err(mudu_error!(ErrorCode::Parse, "frame header is incomplete"));
    }
    let magic = u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]);
    let version = u32::from_be_bytes([buf[4], buf[5], buf[6], buf[7]]);
    Ok((magic, version))
}

#[cfg(test)]
mod mod_test;

#[cfg(test)]
mod latest_test;
