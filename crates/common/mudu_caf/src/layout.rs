//! The binary section layout of a `.mpk` container: a fixed boot header and a
//! fixed-width section directory.
//!
//! All integers are little-endian and sections are 8-byte aligned. Because the
//! manifest carries no offsets and directory entries are fixed-width, the
//! whole layout computes in a single forward pass.

use crate::FORMAT_VERSION;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::{bail, mudu_error};

/// The file magic identifying a `.mpk` artifact.
pub const MAGIC: [u8; 4] = *b"CAF\0";
/// Size of the fixed boot header in bytes.
pub const HEADER_SIZE: u64 = 92;
/// Header flag: the artifact carries an ed25519 signature section.
pub const FLAG_SIGNED: u16 = 0x0001;
/// Header flag: the components / payload / private manifest are encrypted.
pub const FLAG_ENCRYPTED: u16 = 0x0002;
/// Mask of every defined header flag.
pub const KNOWN_FLAGS: u16 = FLAG_SIGNED | FLAG_ENCRYPTED;

/// Directory entry tag: a WASM component section.
pub const TAG_COMPONENT: u8 = 0;
/// Directory entry tag: a payload file section.
pub const TAG_PAYLOAD: u8 = 1;
/// Directory entry tag: the manifest section.
pub const TAG_MANIFEST: u8 = 2;
/// Directory entry tag: the signature section.
pub const TAG_SIGNATURE: u8 = 3;
/// Directory entry tag: the keybox section (encryption).
pub const TAG_KEYBOX: u8 = 4;
/// Directory entry tag: the encrypted private manifest section.
pub const TAG_PRIVATE: u8 = 5;
/// Every tag a v1 reader understands.
pub const KNOWN_TAGS: [u8; 6] = [
    TAG_COMPONENT,
    TAG_PAYLOAD,
    TAG_MANIFEST,
    TAG_SIGNATURE,
    TAG_KEYBOX,
    TAG_PRIVATE,
];

/// Size of a fixed-width directory entry in bytes: `tag u8 + flags u8 +
/// hash_alg u8 + reserved u8 + offset u64 + length u64 + hash [u8;32]`.
pub const DIRECTORY_ENTRY_SIZE: usize = 1 + 1 + 1 + 1 + 8 + 8 + 32;

/// Directory entry flag: the section is compressed (reserved in v1; readers
/// reject it).
pub const ENTRY_FLAG_COMPRESSED: u8 = 0x01;
/// The entry flags a v1 reader accepts.
pub const KNOWN_ENTRY_FLAGS: u8 = 0;

/// Directory entry hash algorithm: SHA-256.
pub const HASH_ALG_SHA256: u8 = 0;

/// One section-directory entry: a typed `(offset, length, hash)` pointer to a
/// section of the artifact.
#[derive(Debug, Clone, Copy)]
pub struct DirectoryEntry {
    /// The section type (`TAG_*`).
    pub tag: u8,
    /// Section attributes (`ENTRY_FLAG_*`).
    pub flags: u8,
    /// The hash algorithm of `hash` (`HASH_ALG_*`).
    pub hash_alg: u8,
    /// Absolute byte offset of the section.
    pub offset: u64,
    /// Byte length of the section.
    pub length: u64,
    /// Hash of the section bytes (SHA-256).
    pub hash: [u8; 32],
}

/// The section layout. Every offset computes in a single forward pass.
pub struct Layout {
    /// Offset of the manifest section.
    pub manifest_offset: u64,
    /// Byte length of the manifest section.
    pub manifest_len: u64,
    /// Offset of the interface section.
    pub interface_offset: u64,
    /// Byte length of the interface section.
    pub interface_len: u64,
    /// Offset of the section directory.
    pub directory_offset: u64,
    /// Byte length of the section directory.
    pub directory_len: u64,
    /// Offset of the keybox section (sealed; `0` when absent).
    pub keybox_offset: u64,
    /// Offset of the encrypted private manifest (sealed; `0` when absent).
    pub private_offset: u64,
    /// Byte length of the encrypted private manifest (sealed; `0` when
    /// absent).
    pub private_len: u64,
    /// Offset of each WASM component (in manifest order).
    pub component_offsets: Vec<u64>,
    /// Offset of each payload file (in DFS order).
    pub file_offsets: Vec<u64>,
    /// Offset of the signature section (`0` when unsigned).
    pub signature_offset: u64,
}

/// Computes the complete layout in a single forward pass, 8-byte aligning
/// every section.
#[allow(clippy::too_many_arguments)]
pub fn compute(
    manifest_len: u64,
    interface_len: u64,
    directory_len: u64,
    component_lens: &[u64],
    file_lens: &[u64],
    signed: bool,
    sealed: Option<(u64, u64)>,
) -> Layout {
    let mut cursor = align_up(HEADER_SIZE, 8);
    let manifest_offset = cursor;
    cursor = align_up(cursor + manifest_len, 8);
    let interface_offset = cursor;
    cursor = align_up(cursor + interface_len, 8);
    let directory_offset = cursor;
    cursor = align_up(cursor + directory_len, 8);

    let (keybox_offset, private_offset, private_len) = match sealed {
        Some((keybox_len, private_len)) => {
            let keybox_offset = cursor;
            cursor = align_up(cursor + keybox_len, 8);
            let private_offset = cursor;
            cursor = align_up(cursor + private_len, 8);
            (keybox_offset, private_offset, private_len)
        }
        None => (0, 0, 0),
    };

    let mut component_offsets = Vec::with_capacity(component_lens.len());
    for len in component_lens {
        component_offsets.push(cursor);
        cursor = align_up(cursor + len, 8);
    }

    let mut file_offsets = Vec::with_capacity(file_lens.len());
    for len in file_lens {
        file_offsets.push(cursor);
        cursor = align_up(cursor + len, 8);
    }

    let signature_offset = if signed { cursor } else { 0 };

    Layout {
        manifest_offset,
        manifest_len,
        interface_offset,
        interface_len,
        directory_offset,
        directory_len,
        keybox_offset,
        private_offset,
        private_len,
        component_offsets,
        file_offsets,
        signature_offset,
    }
}

/// Rounds `v` up to a multiple of `align` (a power of two).
pub fn align_up(v: u64, align: u64) -> u64 {
    (v + align - 1) & !(align - 1)
}

/// Serializes the 92-byte boot header.
pub fn write_header(
    out: &mut Vec<u8>,
    layout: &Layout,
    format_version: u32,
    root_hash: &[u8; 32],
    flags: u16,
) {
    out.extend_from_slice(&MAGIC);
    out.extend_from_slice(&format_version.to_le_bytes());
    out.extend_from_slice(&(HEADER_SIZE as u16).to_le_bytes());
    out.extend_from_slice(&flags.to_le_bytes());
    out.extend_from_slice(&layout.manifest_offset.to_le_bytes());
    out.extend_from_slice(&layout.manifest_len.to_le_bytes());
    out.extend_from_slice(&layout.interface_offset.to_le_bytes());
    out.extend_from_slice(&layout.interface_len.to_le_bytes());
    out.extend_from_slice(&layout.directory_offset.to_le_bytes());
    out.extend_from_slice(&layout.directory_len.to_le_bytes());
    out.extend_from_slice(root_hash);
}

/// Serializes the section directory: a `u32` entry count followed by
/// fixed-width entries.
pub fn encode_directory(entries: &[DirectoryEntry]) -> Vec<u8> {
    let mut out = Vec::with_capacity(4 + entries.len() * DIRECTORY_ENTRY_SIZE);
    out.extend_from_slice(&(entries.len() as u32).to_le_bytes());
    for e in entries {
        out.push(e.tag);
        out.push(e.flags);
        out.push(e.hash_alg);
        out.push(0); // reserved
        out.extend_from_slice(&e.offset.to_le_bytes());
        out.extend_from_slice(&e.length.to_le_bytes());
        out.extend_from_slice(&e.hash);
    }
    out
}

/// Parses the section directory (fail-closed).
pub fn decode_directory(bytes: &[u8]) -> RS<Vec<DirectoryEntry>> {
    let count = le_u32(
        bytes
            .get(0..4)
            .ok_or_else(|| mudu_error!(ErrorCode::CorruptedData, "directory too short"))?,
    )? as usize;
    let expected = 4usize
        .checked_add(
            count
                .checked_mul(DIRECTORY_ENTRY_SIZE)
                .ok_or_else(|| mudu_error!(ErrorCode::CorruptedData, "directory size overflow"))?,
        )
        .ok_or_else(|| mudu_error!(ErrorCode::CorruptedData, "directory size overflow"))?;
    if bytes.len() < expected {
        bail!(ErrorCode::CorruptedData, "directory truncated");
    }
    let mut entries = Vec::with_capacity(count);
    let mut pos = 4usize;
    for _ in 0..count {
        let tag = bytes[pos];
        if !KNOWN_TAGS.contains(&tag) {
            bail!(ErrorCode::CorruptedData, "unknown directory entry tag");
        }
        let flags = bytes[pos + 1];
        if flags & !KNOWN_ENTRY_FLAGS != 0 {
            bail!(ErrorCode::CorruptedData, "unknown directory entry flags");
        }
        let hash_alg = bytes[pos + 2];
        if hash_alg != HASH_ALG_SHA256 {
            bail!(
                ErrorCode::CorruptedData,
                "unknown directory entry hash algorithm"
            );
        }
        if bytes[pos + 3] != 0 {
            bail!(
                ErrorCode::CorruptedData,
                "nonzero reserved byte in directory entry"
            );
        }
        let offset =
            le_u64(bytes.get(pos + 4..pos + 12).ok_or_else(|| {
                mudu_error!(ErrorCode::CorruptedData, "directory entry truncated")
            })?)?;
        let length =
            le_u64(bytes.get(pos + 12..pos + 20).ok_or_else(|| {
                mudu_error!(ErrorCode::CorruptedData, "directory entry truncated")
            })?)?;
        let mut hash = [0u8; 32];
        hash.copy_from_slice(
            bytes.get(pos + 20..pos + 52).ok_or_else(|| {
                mudu_error!(ErrorCode::CorruptedData, "directory entry truncated")
            })?,
        );
        entries.push(DirectoryEntry {
            tag,
            flags,
            hash_alg,
            offset,
            length,
            hash,
        });
        pos += DIRECTORY_ENTRY_SIZE;
    }
    Ok(entries)
}

/// The parsed 92-byte boot header.
pub struct Header {
    /// The container format version.
    pub format_version: u32,
    /// Header flags (`FLAG_*`).
    pub flags: u16,
    /// Offset of the manifest section.
    pub manifest_offset: u64,
    /// Byte length of the manifest section.
    pub manifest_len: u64,
    /// Offset of the interface section.
    pub interface_offset: u64,
    /// Byte length of the interface section.
    pub interface_len: u64,
    /// Offset of the section directory.
    pub directory_offset: u64,
    /// Byte length of the section directory.
    pub directory_len: u64,
    /// The domain-separated artifact root hash.
    pub root_hash: [u8; 32],
}

/// Parses and validates the boot header.
pub fn read_header(bytes: &[u8]) -> RS<Header> {
    if bytes.len() < HEADER_SIZE as usize {
        bail!(ErrorCode::CorruptedData, "file too small for a CAF header");
    }
    if bytes.get(0..4) != Some(&MAGIC[..]) {
        bail!(ErrorCode::CorruptedData, "bad magic (not a .mpk file)");
    }
    let format_version = le_u32(
        bytes
            .get(4..8)
            .ok_or_else(|| mudu_error!(ErrorCode::CorruptedData, "header truncated"))?,
    )?;
    let header_size = le_u16(
        bytes
            .get(8..10)
            .ok_or_else(|| mudu_error!(ErrorCode::CorruptedData, "header truncated"))?,
    )?;
    let flags = le_u16(
        bytes
            .get(10..12)
            .ok_or_else(|| mudu_error!(ErrorCode::CorruptedData, "header truncated"))?,
    )?;
    if flags & !KNOWN_FLAGS != 0 {
        bail!(ErrorCode::CorruptedData, "unknown header flags");
    }
    if format_version != FORMAT_VERSION {
        bail!(
            ErrorCode::UnsupportedFormatVersion,
            "unsupported format version"
        );
    }
    if header_size != HEADER_SIZE as u16 {
        bail!(ErrorCode::CorruptedData, "unsupported header size");
    }
    let manifest_offset = le_u64(
        bytes
            .get(12..20)
            .ok_or_else(|| mudu_error!(ErrorCode::CorruptedData, "header truncated"))?,
    )?;
    let manifest_len = le_u64(
        bytes
            .get(20..28)
            .ok_or_else(|| mudu_error!(ErrorCode::CorruptedData, "header truncated"))?,
    )?;
    let interface_offset = le_u64(
        bytes
            .get(28..36)
            .ok_or_else(|| mudu_error!(ErrorCode::CorruptedData, "header truncated"))?,
    )?;
    let interface_len = le_u64(
        bytes
            .get(36..44)
            .ok_or_else(|| mudu_error!(ErrorCode::CorruptedData, "header truncated"))?,
    )?;
    let directory_offset = le_u64(
        bytes
            .get(44..52)
            .ok_or_else(|| mudu_error!(ErrorCode::CorruptedData, "header truncated"))?,
    )?;
    let directory_len = le_u64(
        bytes
            .get(52..60)
            .ok_or_else(|| mudu_error!(ErrorCode::CorruptedData, "header truncated"))?,
    )?;
    let mut root_hash = [0u8; 32];
    root_hash.copy_from_slice(
        bytes
            .get(60..92)
            .ok_or_else(|| mudu_error!(ErrorCode::CorruptedData, "header truncated"))?,
    );
    Ok(Header {
        format_version,
        flags,
        manifest_offset,
        manifest_len,
        interface_offset,
        interface_len,
        directory_offset,
        directory_len,
        root_hash,
    })
}

/// Reads the `[offset, offset+len)` slice, checking the bounds.
pub fn slice(bytes: &[u8], offset: u64, len: u64) -> RS<&[u8]> {
    let start = usize::try_from(offset)
        .map_err(|_| mudu_error!(ErrorCode::CorruptedData, "offset out of range"))?;
    let end = start
        .checked_add(
            usize::try_from(len)
                .map_err(|_| mudu_error!(ErrorCode::CorruptedData, "length out of range"))?,
        )
        .ok_or_else(|| mudu_error!(ErrorCode::CorruptedData, "section range overflow"))?;
    bytes
        .get(start..end)
        .ok_or_else(|| mudu_error!(ErrorCode::CorruptedData, "section out of bounds"))
}

fn le_u16(b: &[u8]) -> RS<u16> {
    let arr: [u8; 2] = b
        .try_into()
        .map_err(|_| mudu_error!(ErrorCode::CorruptedData, "short u16"))?;
    Ok(u16::from_le_bytes(arr))
}

fn le_u32(b: &[u8]) -> RS<u32> {
    let arr: [u8; 4] = b
        .try_into()
        .map_err(|_| mudu_error!(ErrorCode::CorruptedData, "short u32"))?;
    Ok(u32::from_le_bytes(arr))
}

fn le_u64(b: &[u8]) -> RS<u64> {
    let arr: [u8; 8] = b
        .try_into()
        .map_err(|_| mudu_error!(ErrorCode::CorruptedData, "short u64"))?;
    Ok(u64::from_le_bytes(arr))
}
