//! Canonical MessagePack primitive codec.
//!
//! Byte-level core shared by the MSSP wire runtime ([`crate::universal::mp_wire`]):
//! markers, integers, float bit patterns, and the str/bin/array/map length
//! headers. Encoding is **canonical** (minimal-width markers, big-endian
//! payloads); decoding accepts **any** width of a marker family, not just
//! the minimal one.
//!
//! Composite values (strings, byte strings, arrays, maps) are handled as a
//! decoded length header plus a payload managed by the caller; the recursive
//! value walk and UTF-8 validation live in the caller. Floats cross this
//! boundary as raw big-endian bit patterns so the module stays float-free.
//!
//! Every decoder returns the value together with the number of bytes
//! consumed, so concatenated encodings decode sequentially.
//!
//! This module is self-contained on purpose: it is mirrored verbatim into
//! the standalone guest SDK by `mudu_api_sync`, and it is the extraction
//! target of the formal verification registry (`verification/targets.toml`).

/// A primitive MessagePack decode failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MpError {
    /// The input ended before the value was complete.
    UnexpectedEnd,
    /// The marker byte does not belong to the expected family; `0xC1` is
    /// never valid (it is reserved by the MessagePack specification).
    UnsupportedMarker(u8),
}

const MARKER_NIL: u8 = 0xc0;
const MARKER_FALSE: u8 = 0xc2;
const MARKER_TRUE: u8 = 0xc3;
const MARKER_BIN8: u8 = 0xc4;
const MARKER_BIN16: u8 = 0xc5;
const MARKER_BIN32: u8 = 0xc6;
const MARKER_F32: u8 = 0xca;
const MARKER_F64: u8 = 0xcb;
const MARKER_U8: u8 = 0xcc;
const MARKER_U16: u8 = 0xcd;
const MARKER_U32: u8 = 0xce;
const MARKER_U64: u8 = 0xcf;
const MARKER_I8: u8 = 0xd0;
const MARKER_I16: u8 = 0xd1;
const MARKER_I32: u8 = 0xd2;
const MARKER_I64: u8 = 0xd3;
const MARKER_STR8: u8 = 0xd9;
const MARKER_STR16: u8 = 0xda;
const MARKER_STR32: u8 = 0xdb;
const MARKER_ARRAY16: u8 = 0xdc;
const MARKER_ARRAY32: u8 = 0xdd;
const MARKER_MAP16: u8 = 0xde;
const MARKER_MAP32: u8 = 0xdf;

const FIXSTR_BASE: u8 = 0xa0;
const FIXSTR_MAX_LEN: u32 = 31;
const FIXARRAY_BASE: u8 = 0x90;
const FIXMAP_BASE: u8 = 0x80;
const FIXCONTAINER_MAX_LEN: u32 = 15;

/// Encodes MessagePack nil (`0xC0`).
pub fn write_nil(out: &mut Vec<u8>) {
    out.push(MARKER_NIL);
}

/// Encodes a boolean (`0xC3` for true, `0xC2` for false).
pub fn write_bool(out: &mut Vec<u8>, value: bool) {
    out.push(if value { MARKER_TRUE } else { MARKER_FALSE });
}

/// Encodes an unsigned integer with the minimal-width marker chain:
/// fixpos, then `0xCC`/`0xCD`/`0xCE`/`0xCF` with a big-endian payload.
pub fn write_uint(out: &mut Vec<u8>, value: u64) {
    if value <= 0x7f {
        out.push(value as u8);
    } else if value <= u8::MAX as u64 {
        out.push(MARKER_U8);
        out.push(value as u8);
    } else if value <= u16::MAX as u64 {
        out.push(MARKER_U16);
        out.extend_from_slice(&(value as u16).to_be_bytes());
    } else if value <= u32::MAX as u64 {
        out.push(MARKER_U32);
        out.extend_from_slice(&(value as u32).to_be_bytes());
    } else {
        out.push(MARKER_U64);
        out.extend_from_slice(&value.to_be_bytes());
    }
}

/// Encodes a signed integer. Non-negative values follow the unsigned chain;
/// negative values use the minimal-width signed chain: fixneg, then
/// `0xD0`/`0xD1`/`0xD2`/`0xD3` with a big-endian two's-complement payload.
pub fn write_sint(out: &mut Vec<u8>, value: i64) {
    if value >= 0 {
        write_uint(out, value as u64);
    } else if value >= -32 {
        out.push(value as u8);
    } else if value >= i8::MIN as i64 {
        out.push(MARKER_I8);
        out.push(value as u8);
    } else if value >= i16::MIN as i64 {
        out.push(MARKER_I16);
        out.extend_from_slice(&(value as i16).to_be_bytes());
    } else if value >= i32::MIN as i64 {
        out.push(MARKER_I32);
        out.extend_from_slice(&(value as i32).to_be_bytes());
    } else {
        out.push(MARKER_I64);
        out.extend_from_slice(&value.to_be_bytes());
    }
}

/// Encodes the `0xCA` marker followed by the big-endian bits of an `f32`.
pub fn write_f32_bits(out: &mut Vec<u8>, bits: u32) {
    out.push(MARKER_F32);
    out.extend_from_slice(&bits.to_be_bytes());
}

/// Encodes the `0xCB` marker followed by the big-endian bits of an `f64`.
pub fn write_f64_bits(out: &mut Vec<u8>, bits: u64) {
    out.push(MARKER_F64);
    out.extend_from_slice(&bits.to_be_bytes());
}

/// Encodes a string length header: fixstr, then `0xD9`/`0xDA`/`0xDB`.
pub fn write_str_header(out: &mut Vec<u8>, len: u32) {
    if len <= FIXSTR_MAX_LEN {
        out.push(FIXSTR_BASE + len as u8);
    } else if len <= u8::MAX as u32 {
        out.push(MARKER_STR8);
        out.push(len as u8);
    } else if len <= u16::MAX as u32 {
        out.push(MARKER_STR16);
        out.extend_from_slice(&(len as u16).to_be_bytes());
    } else {
        out.push(MARKER_STR32);
        out.extend_from_slice(&len.to_be_bytes());
    }
}

/// Encodes a byte-string length header: `0xC4`/`0xC5`/`0xC6`.
pub fn write_bin_header(out: &mut Vec<u8>, len: u32) {
    if len <= u8::MAX as u32 {
        out.push(MARKER_BIN8);
        out.push(len as u8);
    } else if len <= u16::MAX as u32 {
        out.push(MARKER_BIN16);
        out.extend_from_slice(&(len as u16).to_be_bytes());
    } else {
        out.push(MARKER_BIN32);
        out.extend_from_slice(&len.to_be_bytes());
    }
}

/// Encodes an array length header: fixarray, then `0xDC`/`0xDD`.
pub fn write_array_header(out: &mut Vec<u8>, len: u32) {
    if len <= FIXCONTAINER_MAX_LEN {
        out.push(FIXARRAY_BASE + len as u8);
    } else if len <= u16::MAX as u32 {
        out.push(MARKER_ARRAY16);
        out.extend_from_slice(&(len as u16).to_be_bytes());
    } else {
        out.push(MARKER_ARRAY32);
        out.extend_from_slice(&len.to_be_bytes());
    }
}

/// Encodes a map length header: fixmap, then `0xDE`/`0xDF`.
pub fn write_map_header(out: &mut Vec<u8>, len: u32) {
    if len <= FIXCONTAINER_MAX_LEN {
        out.push(FIXMAP_BASE + len as u8);
    } else if len <= u16::MAX as u32 {
        out.push(MARKER_MAP16);
        out.extend_from_slice(&(len as u16).to_be_bytes());
    } else {
        out.push(MARKER_MAP32);
        out.extend_from_slice(&len.to_be_bytes());
    }
}

/// Decodes an unsigned integer: fixpos and the `0xCC`..`0xCF` markers of any
/// width are accepted.
pub fn read_uint(input: &[u8]) -> Result<(u64, usize), MpError> {
    if input.is_empty() {
        return Err(MpError::UnexpectedEnd);
    }
    let marker = input[0];
    if marker <= 0x7f {
        return Ok((marker as u64, 1));
    }
    if marker == MARKER_U8 {
        let value = read_be1(input)?;
        return Ok((value as u64, 2));
    }
    if marker == MARKER_U16 {
        let value = read_be2(input)?;
        return Ok((value as u64, 3));
    }
    if marker == MARKER_U32 {
        let value = read_be4(input)?;
        return Ok((value as u64, 5));
    }
    if marker == MARKER_U64 {
        let value = read_be8(input)?;
        return Ok((value, 9));
    }
    Err(MpError::UnsupportedMarker(marker))
}

/// Decodes a signed integer: fixneg and the `0xD0`..`0xD3` markers of any
/// width are accepted.
pub fn read_sint(input: &[u8]) -> Result<(i64, usize), MpError> {
    if input.is_empty() {
        return Err(MpError::UnexpectedEnd);
    }
    let marker = input[0];
    if marker >= 0xe0 {
        return Ok((marker as i8 as i64, 1));
    }
    if marker == MARKER_I8 {
        let value = read_be1(input)?;
        return Ok((value as i8 as i64, 2));
    }
    if marker == MARKER_I16 {
        let value = read_be2(input)?;
        return Ok((value as i16 as i64, 3));
    }
    if marker == MARKER_I32 {
        let value = read_be4(input)?;
        return Ok((value as i32 as i64, 5));
    }
    if marker == MARKER_I64 {
        let value = read_be8(input)?;
        return Ok((value as i64, 9));
    }
    Err(MpError::UnsupportedMarker(marker))
}

/// Decodes a boolean (`0xC2`/`0xC3`).
pub fn read_bool(input: &[u8]) -> Result<(bool, usize), MpError> {
    if input.is_empty() {
        return Err(MpError::UnexpectedEnd);
    }
    let marker = input[0];
    if marker == MARKER_FALSE {
        return Ok((false, 1));
    }
    if marker == MARKER_TRUE {
        return Ok((true, 1));
    }
    Err(MpError::UnsupportedMarker(marker))
}

/// Decodes MessagePack nil (`0xC0`), returning the bytes consumed.
pub fn read_nil(input: &[u8]) -> Result<usize, MpError> {
    if input.is_empty() {
        return Err(MpError::UnexpectedEnd);
    }
    let marker = input[0];
    if marker == MARKER_NIL {
        Ok(1)
    } else {
        Err(MpError::UnsupportedMarker(marker))
    }
}

/// Decodes the `0xCA` marker and returns the big-endian `f32` bit pattern.
pub fn read_f32_bits(input: &[u8]) -> Result<(u32, usize), MpError> {
    if input.is_empty() {
        return Err(MpError::UnexpectedEnd);
    }
    let marker = input[0];
    if marker != MARKER_F32 {
        return Err(MpError::UnsupportedMarker(marker));
    }
    let bits = read_be4(input)?;
    Ok((bits, 5))
}

/// Decodes the `0xCB` marker and returns the big-endian `f64` bit pattern.
pub fn read_f64_bits(input: &[u8]) -> Result<(u64, usize), MpError> {
    if input.is_empty() {
        return Err(MpError::UnexpectedEnd);
    }
    let marker = input[0];
    if marker != MARKER_F64 {
        return Err(MpError::UnsupportedMarker(marker));
    }
    let bits = read_be8(input)?;
    Ok((bits, 9))
}

/// Decodes a string length header: fixstr and the `0xD9`/`0xDA`/`0xDB`
/// markers of any width are accepted.
pub fn read_str_header(input: &[u8]) -> Result<(u32, usize), MpError> {
    if input.is_empty() {
        return Err(MpError::UnexpectedEnd);
    }
    let marker = input[0];
    // Fixstr: the top three bits are 101.
    if marker >> 5 == 0b101 {
        return Ok(((marker % 32) as u32, 1));
    }
    if marker == MARKER_STR8 {
        let len = read_be1(input)?;
        return Ok((len as u32, 2));
    }
    if marker == MARKER_STR16 {
        let len = read_be2(input)?;
        return Ok((len as u32, 3));
    }
    if marker == MARKER_STR32 {
        let len = read_be4(input)?;
        return Ok((len, 5));
    }
    Err(MpError::UnsupportedMarker(marker))
}

/// Decodes a byte-string length header (`0xC4`/`0xC5`/`0xC6`).
pub fn read_bin_header(input: &[u8]) -> Result<(u32, usize), MpError> {
    if input.is_empty() {
        return Err(MpError::UnexpectedEnd);
    }
    let marker = input[0];
    if marker == MARKER_BIN8 {
        let len = read_be1(input)?;
        return Ok((len as u32, 2));
    }
    if marker == MARKER_BIN16 {
        let len = read_be2(input)?;
        return Ok((len as u32, 3));
    }
    if marker == MARKER_BIN32 {
        let len = read_be4(input)?;
        return Ok((len, 5));
    }
    Err(MpError::UnsupportedMarker(marker))
}

/// Decodes an array length header: fixarray and `0xDC`/`0xDD` are accepted.
pub fn read_array_header(input: &[u8]) -> Result<(u32, usize), MpError> {
    if input.is_empty() {
        return Err(MpError::UnexpectedEnd);
    }
    let marker = input[0];
    // Fixarray: the top four bits are 1001.
    if marker >> 4 == 0b1001 {
        return Ok(((marker % 16) as u32, 1));
    }
    if marker == MARKER_ARRAY16 {
        let len = read_be2(input)?;
        return Ok((len as u32, 3));
    }
    if marker == MARKER_ARRAY32 {
        let len = read_be4(input)?;
        return Ok((len, 5));
    }
    Err(MpError::UnsupportedMarker(marker))
}

/// Decodes a map length header: fixmap and `0xDE`/`0xDF` are accepted.
pub fn read_map_header(input: &[u8]) -> Result<(u32, usize), MpError> {
    if input.is_empty() {
        return Err(MpError::UnexpectedEnd);
    }
    let marker = input[0];
    // Fixmap: the top four bits are 1000.
    if marker >> 4 == 0b1000 {
        return Ok(((marker % 16) as u32, 1));
    }
    if marker == MARKER_MAP16 {
        let len = read_be2(input)?;
        return Ok((len as u32, 3));
    }
    if marker == MARKER_MAP32 {
        let len = read_be4(input)?;
        return Ok((len, 5));
    }
    Err(MpError::UnsupportedMarker(marker))
}

/// Reads the big-endian byte at offset 1 (the marker is at offset 0).
fn read_be1(input: &[u8]) -> Result<u8, MpError> {
    if input.len() < 2 {
        return Err(MpError::UnexpectedEnd);
    }
    Ok(input[1])
}

/// Reads the big-endian `u16` payload following the marker byte.
fn read_be2(input: &[u8]) -> Result<u16, MpError> {
    if input.len() < 3 {
        return Err(MpError::UnexpectedEnd);
    }
    Ok((input[1] as u16) * 256 + (input[2] as u16))
}

/// Reads the big-endian `u32` payload following the marker byte.
fn read_be4(input: &[u8]) -> Result<u32, MpError> {
    if input.len() < 5 {
        return Err(MpError::UnexpectedEnd);
    }
    Ok((input[1] as u32) * 16777216
        + (input[2] as u32) * 65536
        + (input[3] as u32) * 256
        + (input[4] as u32))
}

/// Reads the big-endian `u64` payload following the marker byte.
fn read_be8(input: &[u8]) -> Result<u64, MpError> {
    if input.len() < 9 {
        return Err(MpError::UnexpectedEnd);
    }
    Ok((input[1] as u64) * 72057594037927936
        + (input[2] as u64) * 281474976710656
        + (input[3] as u64) * 1099511627776
        + (input[4] as u64) * 4294967296
        + (input[5] as u64) * 16777216
        + (input[6] as u64) * 65536
        + (input[7] as u64) * 256
        + (input[8] as u64))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encoded(f: impl Fn(&mut Vec<u8>)) -> Vec<u8> {
        let mut out = Vec::new();
        f(&mut out);
        out
    }

    #[test]
    fn uint_encodings_are_minimal_width() {
        assert_eq!(encoded(|o| write_uint(o, 0)), &[0x00]);
        assert_eq!(encoded(|o| write_uint(o, 127)), &[0x7f]);
        assert_eq!(encoded(|o| write_uint(o, 128)), &[0xcc, 0x80]);
        assert_eq!(encoded(|o| write_uint(o, 255)), &[0xcc, 0xff]);
        assert_eq!(encoded(|o| write_uint(o, 256)), &[0xcd, 0x01, 0x00]);
        assert_eq!(encoded(|o| write_uint(o, 65535)), &[0xcd, 0xff, 0xff]);
        assert_eq!(
            encoded(|o| write_uint(o, 65536)),
            &[0xce, 0x00, 0x01, 0x00, 0x00]
        );
        assert_eq!(
            encoded(|o| write_uint(o, u32::MAX as u64 + 1)),
            &[0xcf, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00]
        );
        assert_eq!(
            encoded(|o| write_uint(o, u64::MAX)),
            &[0xcf, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff]
        );
    }

    #[test]
    fn sint_encodings_are_minimal_width() {
        assert_eq!(encoded(|o| write_sint(o, 0)), &[0x00]);
        assert_eq!(encoded(|o| write_sint(o, 127)), &[0x7f]);
        assert_eq!(encoded(|o| write_sint(o, 128)), &[0xcc, 0x80]);
        assert_eq!(encoded(|o| write_sint(o, -1)), &[0xff]);
        assert_eq!(encoded(|o| write_sint(o, -32)), &[0xe0]);
        assert_eq!(encoded(|o| write_sint(o, -33)), &[0xd0, 0xdf]);
        assert_eq!(encoded(|o| write_sint(o, -128)), &[0xd0, 0x80]);
        assert_eq!(encoded(|o| write_sint(o, -129)), &[0xd1, 0xff, 0x7f]);
        assert_eq!(encoded(|o| write_sint(o, -32768)), &[0xd1, 0x80, 0x00]);
        assert_eq!(
            encoded(|o| write_sint(o, -32769)),
            &[0xd2, 0xff, 0xff, 0x7f, 0xff]
        );
        assert_eq!(
            encoded(|o| write_sint(o, i64::MIN)),
            &[0xd3, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]
        );
    }

    #[test]
    fn uint_roundtrips_at_every_width() {
        for (bytes, value, used) in [
            (vec![0x00], 0, 1),
            (vec![0x7f], 127, 1),
            (vec![0xcc, 0x80], 128, 2),
            (vec![0xcd, 0x01, 0x00], 256, 3),
            (vec![0xce, 0x00, 0x01, 0x00, 0x00], 65536, 5),
            (
                vec![0xcf, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff],
                u64::MAX,
                9,
            ),
        ] {
            assert_eq!(read_uint(&bytes), Ok((value, used)));
        }
        // Non-minimal widths are accepted on decode.
        assert_eq!(read_uint(&[0xcd, 0x00, 0x05]), Ok((5, 3)));
        assert_eq!(read_uint(&[0xce, 0x00, 0x00, 0x00, 0x05]), Ok((5, 5)));
    }

    #[test]
    fn sint_roundtrips_at_every_width() {
        for (bytes, value, used) in [
            (vec![0xff], -1, 1),
            (vec![0xe0], -32, 1),
            (vec![0xd0, 0xdf], -33, 2),
            (vec![0xd1, 0xff, 0x7f], -129, 3),
            (vec![0xd2, 0xff, 0xff, 0x7f, 0xff], -32769, 5),
            (
                vec![0xd3, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
                i64::MIN,
                9,
            ),
        ] {
            assert_eq!(read_sint(&bytes), Ok((value, used)));
        }
        assert_eq!(read_sint(&[0xd0, 0x80]), Ok((-128, 2)));
    }

    #[test]
    fn nil_bool_and_floats_roundtrip() {
        assert_eq!(encoded(write_nil), &[0xc0]);
        assert_eq!(read_nil(&[0xc0]), Ok(1));
        assert_eq!(encoded(|o| write_bool(o, true)), &[0xc3]);
        assert_eq!(encoded(|o| write_bool(o, false)), &[0xc2]);
        assert_eq!(read_bool(&[0xc3]), Ok((true, 1)));
        assert_eq!(read_bool(&[0xc2]), Ok((false, 1)));

        let f32_bits = 0.1f32.to_bits();
        assert_eq!(
            encoded(|o| write_f32_bits(o, f32_bits)),
            &[0xca, 0x3d, 0xcc, 0xcc, 0xcd]
        );
        assert_eq!(
            read_f32_bits(&[0xca, 0x3d, 0xcc, 0xcc, 0xcd]),
            Ok((f32_bits, 5))
        );
        let f64_bits = (-2.5f64).to_bits();
        assert_eq!(
            encoded(|o| write_f64_bits(o, f64_bits)),
            &[0xcb, 0xc0, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]
        );
        assert_eq!(
            read_f64_bits(&[0xcb, 0xc0, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]),
            Ok((f64_bits, 9))
        );
    }

    #[test]
    fn length_headers_choose_markers() {
        assert_eq!(encoded(|o| write_str_header(o, 31)), &[0xbf]);
        assert_eq!(encoded(|o| write_str_header(o, 32)), &[0xd9, 32]);
        assert_eq!(encoded(|o| write_str_header(o, 255)), &[0xd9, 0xff]);
        assert_eq!(encoded(|o| write_str_header(o, 256)), &[0xda, 0x01, 0x00]);
        assert_eq!(encoded(|o| write_bin_header(o, 255)), &[0xc4, 0xff]);
        assert_eq!(encoded(|o| write_bin_header(o, 256)), &[0xc5, 0x01, 0x00]);
        assert_eq!(encoded(|o| write_array_header(o, 0)), &[0x90]);
        assert_eq!(encoded(|o| write_array_header(o, 15)), &[0x9f]);
        assert_eq!(encoded(|o| write_array_header(o, 16)), &[0xdc, 0x00, 0x10]);
        assert_eq!(encoded(|o| write_map_header(o, 0)), &[0x80]);
        assert_eq!(encoded(|o| write_map_header(o, 15)), &[0x8f]);
        assert_eq!(encoded(|o| write_map_header(o, 16)), &[0xde, 0x00, 0x10]);
    }

    #[test]
    fn length_headers_roundtrip_at_any_width() {
        assert_eq!(read_str_header(&[0xbf]), Ok((31, 1)));
        assert_eq!(read_str_header(&[0xd9, 32]), Ok((32, 2)));
        assert_eq!(read_str_header(&[0xda, 0x01, 0x00]), Ok((256, 3)));
        assert_eq!(
            read_str_header(&[0xdb, 0x00, 0x01, 0x00, 0x00]),
            Ok((65536, 5))
        );
        // Non-minimal widths are accepted on decode.
        assert_eq!(read_str_header(&[0xda, 0x00, 0x05]), Ok((5, 3)));
        assert_eq!(read_bin_header(&[0xc4, 0xff]), Ok((255, 2)));
        assert_eq!(read_bin_header(&[0xc5, 0x01, 0x00]), Ok((256, 3)));
        assert_eq!(
            read_bin_header(&[0xc6, 0x00, 0x01, 0x00, 0x00]),
            Ok((65536, 5))
        );
        assert_eq!(read_array_header(&[0x9f]), Ok((15, 1)));
        assert_eq!(read_array_header(&[0xdc, 0x00, 0x10]), Ok((16, 3)));
        assert_eq!(
            read_array_header(&[0xdd, 0x00, 0x01, 0x00, 0x00]),
            Ok((65536, 5))
        );
        assert_eq!(read_map_header(&[0x8f]), Ok((15, 1)));
        assert_eq!(read_map_header(&[0xde, 0x00, 0x10]), Ok((16, 3)));
        assert_eq!(
            read_map_header(&[0xdf, 0x00, 0x01, 0x00, 0x00]),
            Ok((65536, 5))
        );
    }

    #[test]
    fn decoders_consume_exactly_the_encoding() {
        let mut bytes = encoded(|o| write_uint(o, 128));
        bytes.extend_from_slice(&[0x01, 0x02]);
        assert_eq!(read_uint(&bytes), Ok((128, 2)));

        let mut bytes = encoded(|o| write_str_header(o, 32));
        bytes.extend_from_slice(&[0x00; 32]);
        assert_eq!(read_str_header(&bytes), Ok((32, 2)));
    }

    #[test]
    fn short_inputs_are_rejected() {
        assert_eq!(read_uint(&[]), Err(MpError::UnexpectedEnd));
        assert_eq!(read_uint(&[0xcc]), Err(MpError::UnexpectedEnd));
        assert_eq!(read_uint(&[0xcd, 0x01]), Err(MpError::UnexpectedEnd));
        assert_eq!(
            read_uint(&[0xce, 0x00, 0x01, 0x00]),
            Err(MpError::UnexpectedEnd)
        );
        assert_eq!(
            read_uint(&[0xcf, 0, 0, 0, 0, 0, 0, 0]),
            Err(MpError::UnexpectedEnd)
        );
        assert_eq!(read_sint(&[0xd0]), Err(MpError::UnexpectedEnd));
        assert_eq!(
            read_f32_bits(&[0xca, 0x3d, 0xcc, 0xcc]),
            Err(MpError::UnexpectedEnd)
        );
        assert_eq!(read_str_header(&[0xd9]), Err(MpError::UnexpectedEnd));
        assert_eq!(read_bin_header(&[0xc5, 0x01]), Err(MpError::UnexpectedEnd));
    }

    #[test]
    fn foreign_and_reserved_markers_are_rejected() {
        // 0xC1 is reserved by the MessagePack specification.
        assert_eq!(read_uint(&[0xc1]), Err(MpError::UnsupportedMarker(0xc1)));
        assert_eq!(read_sint(&[0xc1]), Err(MpError::UnsupportedMarker(0xc1)));
        assert_eq!(read_bool(&[0xc1]), Err(MpError::UnsupportedMarker(0xc1)));
        assert_eq!(read_nil(&[0xc1]), Err(MpError::UnsupportedMarker(0xc1)));
        assert_eq!(
            read_str_header(&[0xc1]),
            Err(MpError::UnsupportedMarker(0xc1))
        );
        // A signed marker is not an unsigned one and vice versa.
        assert_eq!(
            read_uint(&[0xd0, 0x00]),
            Err(MpError::UnsupportedMarker(0xd0))
        );
        assert_eq!(
            read_sint(&[0xcc, 0x00]),
            Err(MpError::UnsupportedMarker(0xcc))
        );
        assert_eq!(read_nil(&[0xc3]), Err(MpError::UnsupportedMarker(0xc3)));
        assert_eq!(read_bool(&[0xc0]), Err(MpError::UnsupportedMarker(0xc0)));
        // Negative fixint is not an unsigned integer; positive fixint is not signed.
        assert_eq!(read_uint(&[0xff]), Err(MpError::UnsupportedMarker(0xff)));
        assert_eq!(read_sint(&[0x7f]), Err(MpError::UnsupportedMarker(0x7f)));
    }
}
