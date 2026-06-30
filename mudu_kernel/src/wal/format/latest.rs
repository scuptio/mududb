use crate::wal::lsn::LSN;
use mudu::common::crc::calc_crc;
use mudu::common::result::RS;
use mudu::compat::{self, FormatKind, LOG_FRAME_CURRENT_VERSION};
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_compat_migrate::{global, NoopOptionProvider};
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::borrow::Cow;
use std::sync::atomic::{AtomicU64, Ordering};

const LOG_FRAME_MAGIC: u32 = 0x4C47_464D; // "LGFM"
/// Current version of the log frame format.
///
/// The canonical value lives in [`mudu::compat::LOG_FRAME_CURRENT_VERSION`].
pub const VERSION: u32 = LOG_FRAME_CURRENT_VERSION;
pub const LOG_FRAME_HEADER_SIZE: usize = 24;
pub const LOG_FRAME_TAILER_SIZE: usize = 8;

#[derive(Debug, Eq, PartialEq, Clone, Copy)]
pub struct LogFrameHeader {
    magic: u32,
    version: u32,
    lsn: LSN,
    size: u32,
    n_part: u32,
}

#[derive(Debug, Eq, PartialEq, Clone, Copy)]
pub struct LogFrameTailer {
    n_part: u32,
    checksum: u32,
}

impl LogFrameHeader {
    fn new(lsn: LSN, n_part: u32, size: usize) -> Self {
        Self {
            magic: LOG_FRAME_MAGIC,
            version: VERSION,
            lsn,
            n_part,
            size: size as u32,
        }
    }

    pub fn version(&self) -> u32 {
        self.version
    }

    pub fn n_part(&self) -> u32 {
        self.n_part
    }

    pub fn lsn(&self) -> LSN {
        self.lsn
    }

    pub fn size(&self) -> u32 {
        self.size
    }

    fn encode(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.magic.to_be_bytes());
        out.extend_from_slice(&self.version.to_be_bytes());
        out.extend_from_slice(&self.lsn.as_u64().to_be_bytes());
        out.extend_from_slice(&self.size.to_be_bytes());
        out.extend_from_slice(&self.n_part.to_be_bytes());
    }

    pub(crate) fn decode(input: &[u8]) -> RS<Self> {
        if input.len() < LOG_FRAME_HEADER_SIZE {
            return Err(mudu_error!(
                ErrorCode::Decode,
                "log frame header is truncated"
            ));
        }
        let header = peek_header(input)?;
        compat::check_version(FormatKind::LogFrame, header.version)?;
        Ok(header)
    }
}

fn peek_header(input: &[u8]) -> RS<LogFrameHeader> {
    if input.len() < LOG_FRAME_HEADER_SIZE {
        return Err(mudu_error!(
            ErrorCode::Decode,
            "log frame header is truncated"
        ));
    }
    let magic = u32::from_be_bytes(input[0..4].try_into().unwrap_or([0; 4]));
    let version = u32::from_be_bytes(input[4..8].try_into().unwrap_or([0; 4]));
    let lsn = u64::from_be_bytes(input[8..16].try_into().unwrap_or([0; 8]));
    let size = u32::from_be_bytes(input[16..20].try_into().unwrap_or([0; 4]));
    let n_part = u32::from_be_bytes(input[20..24].try_into().unwrap_or([0; 4]));
    compat::check_magic(FormatKind::LogFrame, magic)?;
    Ok(LogFrameHeader {
        magic,
        version,
        lsn: lsn.into(),
        size,
        n_part,
    })
}

impl LogFrameTailer {
    fn new(n_part: u32, payload: &[u8]) -> Self {
        Self {
            n_part,
            checksum: payload_checksum(payload),
        }
    }

    pub fn n_part(&self) -> u32 {
        self.n_part
    }

    pub fn checksum(&self) -> u32 {
        self.checksum
    }

    fn encode(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.n_part.to_be_bytes());
        out.extend_from_slice(&self.checksum.to_be_bytes());
    }

    fn decode(input: &[u8]) -> RS<Self> {
        if input.len() < LOG_FRAME_TAILER_SIZE {
            return Err(mudu_error!(
                ErrorCode::Decode,
                "log frame tailer is truncated"
            ));
        }
        let n_part = u32::from_be_bytes(input[0..4].try_into().unwrap_or([0; 4]));
        let checksum = u32::from_be_bytes(input[4..8].try_into().unwrap_or([0; 4]));
        Ok(Self { n_part, checksum })
    }
}

pub fn serialize_entry<L: Serialize>(
    value: &L,
    max_part_size: usize,
    next_lsn: &AtomicU64,
) -> RS<Vec<Vec<u8>>> {
    let payload = rmp_serde::to_vec(value)
        .map_err(|e| mudu_error!(ErrorCode::Encode, "encode log entry to msgpack error", e))?;
    if max_part_size <= LOG_FRAME_HEADER_SIZE + LOG_FRAME_TAILER_SIZE {
        return Err(mudu_error!(
            ErrorCode::Parse,
            "max_part_size must be larger than header + tailer"
        ));
    }

    let max_payload_size = max_part_size - LOG_FRAME_HEADER_SIZE - LOG_FRAME_TAILER_SIZE;
    let total_parts = payload.len().div_ceil(max_payload_size).max(1);
    let mut result = Vec::with_capacity(total_parts);
    for (index, chunk) in payload.chunks(max_payload_size).enumerate() {
        let remaining = (total_parts - index - 1) as u32;
        let lsn: LSN = next_lsn.fetch_add(1, Ordering::SeqCst).into();
        let header = LogFrameHeader::new(lsn, remaining, chunk.len());
        let tailer = LogFrameTailer::new(remaining, chunk);
        let mut frame =
            Vec::with_capacity(LOG_FRAME_HEADER_SIZE + chunk.len() + LOG_FRAME_TAILER_SIZE);
        header.encode(&mut frame);
        frame.extend_from_slice(chunk);
        tailer.encode(&mut frame);
        result.push(frame);
    }

    Ok(result)
}

pub fn frame_lsn(frame: &[u8]) -> RS<LSN> {
    Ok(split_frame(frame)?.0.lsn())
}

pub fn frame_lsns(frames: &[Vec<u8>]) -> RS<Vec<LSN>> {
    frames.iter().map(|frame| frame_lsn(frame)).collect()
}

pub fn last_frame_lsn(frames: &[Vec<u8>]) -> RS<LSN> {
    let frame = frames
        .last()
        .ok_or_else(|| mudu_error!(ErrorCode::Decode, "log frames are empty"))?;
    frame_lsn(frame)
}

pub fn deserialize_entry<L: DeserializeOwned>(parts: &[Vec<u8>]) -> RS<L> {
    let payload = deserialize_frames_payload(parts)?;
    rmp_serde::from_slice(&payload)
        .map_err(|e| mudu_error!(ErrorCode::Decode, "decode log entry from msgpack error", e))
}

pub fn split_frame(frame: &[u8]) -> RS<(LogFrameHeader, Vec<u8>, LogFrameTailer)> {
    split_frame_exact(frame)
}

pub fn frame_len(input: &[u8]) -> RS<usize> {
    if input.len() < LOG_FRAME_HEADER_SIZE + LOG_FRAME_TAILER_SIZE {
        return Err(mudu_error!(ErrorCode::Decode, "log frame is truncated"));
    }
    let header = peek_header(&input[..LOG_FRAME_HEADER_SIZE])?;
    let payload_end = LOG_FRAME_HEADER_SIZE + header.size as usize;
    let expected_len = payload_end + LOG_FRAME_TAILER_SIZE;
    if input.len() < expected_len {
        return Err(mudu_error!(
            ErrorCode::Decode,
            format!(
                "log frame is truncated, expected at least {}, got {}",
                expected_len,
                input.len()
            )
        ));
    }
    Ok(expected_len)
}

pub fn decode_entries_with_pending<L: DeserializeOwned>(
    frames: &[Vec<u8>],
    pending_frames: &mut Vec<Vec<u8>>,
    pending_start_lsn: &mut Option<LSN>,
) -> RS<Vec<(LSN, L)>> {
    let mut result = Vec::new();
    if pending_start_lsn.is_none() && !pending_frames.is_empty() {
        let (header, _, _) = split_frame(&pending_frames[0])?;
        *pending_start_lsn = Some(header.lsn());
    }
    for frame in frames {
        let (header, _, _) = split_frame(frame)?;
        if pending_frames.is_empty() {
            *pending_start_lsn = Some(header.lsn());
        }
        pending_frames.push(frame.clone());
        if header.n_part() != 0 {
            continue;
        }

        let entry = deserialize_entry(pending_frames)?;
        let start_lsn = pending_start_lsn.take().ok_or_else(|| {
            mudu_error!(
                ErrorCode::Internal,
                "missing starting lsn for decoded log entry"
            )
        })?;
        pending_frames.clear();
        result.push((start_lsn, entry));
    }
    Ok(result)
}

fn ensure_latest_frame(frame: &[u8]) -> RS<Cow<'_, [u8]>> {
    let header = peek_header(&frame[..LOG_FRAME_HEADER_SIZE])?;
    compat::check_version(FormatKind::LogFrame, header.version)?;
    if header.version == VERSION {
        return Ok(Cow::Borrowed(frame));
    }
    global::upgrade_to_current(
        FormatKind::LogFrame,
        header.version,
        frame,
        &NoopOptionProvider,
    )
    .map(Cow::Owned)
    .map_err(|e| e.into_mudu_error())
}

fn split_frame_exact(frame: &[u8]) -> RS<(LogFrameHeader, Vec<u8>, LogFrameTailer)> {
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
    let migrated = ensure_latest_frame(frame)?;
    let header = LogFrameHeader::decode(&migrated[..LOG_FRAME_HEADER_SIZE])?;
    let payload_end = LOG_FRAME_HEADER_SIZE + header.size() as usize;
    let payload = migrated[LOG_FRAME_HEADER_SIZE..payload_end].to_vec();
    let tailer = LogFrameTailer::decode(&migrated[payload_end..])?;
    if header.n_part() != tailer.n_part() {
        return Err(mudu_error!(
            ErrorCode::Decode,
            "log frame header/tailer n_part mismatch"
        ));
    }
    if payload_checksum(&payload) != tailer.checksum() {
        return Err(mudu_error!(
            ErrorCode::Decode,
            "log frame payload checksum mismatch"
        ));
    }
    Ok((header, payload, tailer))
}

fn deserialize_frames_payload(frames: &[Vec<u8>]) -> RS<Vec<u8>> {
    if frames.is_empty() {
        return Err(mudu_error!(ErrorCode::Decode, "log frames are empty"));
    }

    let mut payload = Vec::new();
    let total_parts = frames.len();
    for (index, frame) in frames.iter().enumerate() {
        let (header, body, _tailer) = split_frame(frame)?;
        let expected_remaining = (total_parts - index - 1) as u32;
        if header.n_part() != expected_remaining {
            return Err(mudu_error!(
                ErrorCode::Decode,
                format!(
                    "unexpected log frame order, expected remaining {}, got {}",
                    expected_remaining,
                    header.n_part()
                )
            ));
        }
        payload.extend_from_slice(&body);
    }
    Ok(payload)
}

fn payload_checksum(payload: &[u8]) -> u32 {
    calc_crc(payload) as u32
}
