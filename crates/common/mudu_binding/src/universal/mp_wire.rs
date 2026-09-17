//! MSSP MessagePack wire runtime: the single hand-written codec core shared
//! by every generated universal type.
//!
//! The guest↔host syscall frames (MSSP v1) carry one MessagePack body whose
//! values are built from a small dynamic model, [`Value`]. Generated record
//! DTOs convert to/from [`Value`] (`to_value` / `from_value`, see the
//! [`ToValue`] / [`FromValue`] traits) and the frame layer encodes the value
//! with [`encode_value`] and parses it with [`decode_value`].
//!
//! Encoding is **canonical** and byte-identical to `rmp_serde` 1.3.1:
//!
//! - integers use minimal-width markers by value; non-negative signed values
//!   follow the unsigned marker chain (`128i64` encodes as `0xCC 0x80`);
//! - strings use fixstr / str8 / str16 / str32; byte strings use bin8 /
//!   bin16 / bin32; arrays use fixarray / array16 / array32; maps use fixmap
//!   / map16 / map32;
//! - `f32`/`f64` use the `0xCA`/`0xCB` markers, nil is `0xC0`, booleans are
//!   `0xC2`/`0xC3`.
//!
//! Decoding is **lenient** (proto3-flavored): integers of any MessagePack
//! width are accepted (with a range check on narrowing), `f32`/`f64` markers
//! are interchangeable, and bin fields additionally accept the array-of-int
//! form. Structural rules (a map where a map is expected, exact tuple
//! lengths, unknown variant tags) stay strict. [`decode_value`] rejects
//! trailing bytes after the top-level value.

use rmp::Marker;
use rmp::decode::read_marker;
use rmp::encode::{
    write_array_len, write_bin, write_bool, write_f32, write_f64, write_map_len, write_nil,
    write_sint, write_str, write_uint,
};

/// Dynamic MessagePack value exchanged on the syscall wire.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// MessagePack nil (`0xC0`); the encoding of `option` None.
    Nil,
    /// Boolean.
    Bool(bool),
    /// Unsigned integer (any width on decode, minimal width on encode).
    UInt(u64),
    /// Signed integer; non-negative values encode through the unsigned chain.
    Int(i64),
    /// 32-bit float.
    F32(f32),
    /// 64-bit float.
    F64(f64),
    /// UTF-8 string.
    Str(String),
    /// Byte string (MessagePack bin).
    Bin(Vec<u8>),
    /// Array (variants `[tag, payload]`, lists, tuples).
    Array(Vec<Value>),
    /// Map (records keyed by 1-based field number, map-shaped envelopes).
    Map(Vec<(Value, Value)>),
}

/// A decode (or type-conversion) failure against the wire model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WireError {
    /// Human-readable description of the violation.
    pub message: String,
}

impl WireError {
    /// Build an error from a message.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for WireError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for WireError {}

fn wire_err<T>(message: impl Into<String>) -> Result<T, WireError> {
    Err(WireError::new(message))
}

/// Maximum nesting depth accepted by the decoder (mirrors the practical
/// limits of `rmp_serde` and guards against stack exhaustion on hostile
/// input).
const MAX_DEPTH: usize = 128;

/// Encodes a [`Value`] into its canonical MessagePack byte form.
pub fn encode_value(value: &Value) -> Vec<u8> {
    let mut out = Vec::new();
    // Writing to a Vec is infallible; the marker-level rmp functions only
    // fail on I/O or length overflow, neither of which can happen here.
    let _ = write_value(&mut out, value);
    out
}

fn write_value(out: &mut Vec<u8>, value: &Value) -> Result<(), rmp::encode::ValueWriteError> {
    match value {
        Value::Nil => write_nil(out).map_err(rmp::encode::ValueWriteError::InvalidDataWrite)?,
        Value::Bool(b) => {
            write_bool(out, *b).map_err(rmp::encode::ValueWriteError::InvalidDataWrite)?
        }
        Value::UInt(v) => {
            write_uint(out, *v)?;
        }
        Value::Int(v) => {
            write_sint(out, *v)?;
        }
        Value::F32(v) => write_f32(out, *v)?,
        Value::F64(v) => write_f64(out, *v)?,
        Value::Str(s) => write_str(out, s)?,
        Value::Bin(bytes) => write_bin(out, bytes)?,
        Value::Array(items) => {
            write_array_len(out, items.len() as u32)?;
            for item in items {
                write_value(out, item)?;
            }
        }
        Value::Map(pairs) => {
            write_map_len(out, pairs.len() as u32)?;
            for (key, val) in pairs {
                write_value(out, key)?;
                write_value(out, val)?;
            }
        }
    }
    Ok(())
}

/// Decodes one [`Value`] from `bytes`, rejecting trailing bytes.
pub fn decode_value(bytes: &[u8]) -> Result<Value, WireError> {
    let (value, used) = decode_value_prefix(bytes)?;
    if used != bytes.len() {
        return wire_err(format!(
            "trailing bytes after MessagePack value ({used} of {} consumed)",
            bytes.len()
        ));
    }
    Ok(value)
}

/// Decodes one [`Value`] from the start of `bytes`, returning the value and
/// the number of bytes consumed.
pub fn decode_value_prefix(bytes: &[u8]) -> Result<(Value, usize), WireError> {
    let mut cursor = bytes;
    let value = read_value(&mut cursor, 0)?;
    Ok((value, bytes.len() - cursor.len()))
}

type SliceCursor<'a> = &'a [u8];

fn cursor_pos(base_len: usize, cursor: &SliceCursor) -> usize {
    base_len - cursor.len()
}

fn read_value(cursor: &mut SliceCursor, depth: usize) -> Result<Value, WireError> {
    if depth > MAX_DEPTH {
        return wire_err("MessagePack value nesting too deep");
    }
    let base_len = cursor.len();
    let marker = read_marker(cursor).map_err(|e| {
        WireError::new(format!(
            "MessagePack marker read error at {} bytes: {e:?}",
            cursor_pos(base_len, cursor)
        ))
    })?;
    match marker {
        Marker::FixPos(v) => Ok(Value::UInt(v as u64)),
        Marker::FixNeg(v) => Ok(Value::Int(v as i64)),
        Marker::Null => Ok(Value::Nil),
        Marker::False => Ok(Value::Bool(false)),
        Marker::True => Ok(Value::Bool(true)),
        Marker::U8 => Ok(Value::UInt(read_be(cursor, 1)?)),
        Marker::U16 => Ok(Value::UInt(read_be(cursor, 2)?)),
        Marker::U32 => Ok(Value::UInt(read_be(cursor, 4)?)),
        Marker::U64 => Ok(Value::UInt(read_be(cursor, 8)?)),
        Marker::I8 => Ok(Value::Int(read_be(cursor, 1)? as u8 as i8 as i64)),
        Marker::I16 => Ok(Value::Int(read_be(cursor, 2)? as u16 as i16 as i64)),
        Marker::I32 => Ok(Value::Int(read_be(cursor, 4)? as u32 as i32 as i64)),
        Marker::I64 => Ok(Value::Int(read_be(cursor, 8)? as i64)),
        Marker::F32 => {
            let b = read_bytes(cursor, 4)?;
            Ok(Value::F32(f32::from_be_bytes([b[0], b[1], b[2], b[3]])))
        }
        Marker::F64 => {
            let b = read_bytes(cursor, 8)?;
            Ok(Value::F64(f64::from_be_bytes([
                b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7],
            ])))
        }
        Marker::FixStr(_) | Marker::Str8 | Marker::Str16 | Marker::Str32 => {
            // The marker has already been consumed; read_str_len expects the
            // marker first, so recompute the length from the marker directly.
            let len = match marker {
                Marker::FixStr(len) => len as u32,
                _ => read_str_len_after_marker(cursor, marker)?,
            };
            let bytes = read_bytes(cursor, len as usize)?;
            let text = String::from_utf8(bytes.to_vec())
                .map_err(|e| WireError::new(format!("invalid UTF-8 string: {e}")))?;
            Ok(Value::Str(text))
        }
        Marker::Bin8 | Marker::Bin16 | Marker::Bin32 => {
            let len = read_bin_len_after_marker(cursor, marker)?;
            let bytes = read_bytes(cursor, len as usize)?;
            Ok(Value::Bin(bytes.to_vec()))
        }
        Marker::FixArray(len) => read_array_items(cursor, len as u32, depth),
        Marker::Array16 | Marker::Array32 => {
            let len = read_array_len_after_marker(cursor, marker)?;
            read_array_items(cursor, len, depth)
        }
        Marker::FixMap(len) => read_map_pairs(cursor, len as u32, depth),
        Marker::Map16 | Marker::Map32 => {
            let len = read_map_len_after_marker(cursor, marker)?;
            read_map_pairs(cursor, len, depth)
        }
        other => wire_err(format!("unsupported MessagePack marker {other:?}")),
    }
}

fn read_array_items(cursor: &mut SliceCursor, len: u32, depth: usize) -> Result<Value, WireError> {
    let mut items = Vec::with_capacity(len as usize);
    for _ in 0..len {
        items.push(read_value(cursor, depth + 1)?);
    }
    Ok(Value::Array(items))
}

fn read_map_pairs(cursor: &mut SliceCursor, len: u32, depth: usize) -> Result<Value, WireError> {
    let mut pairs = Vec::with_capacity(len as usize);
    for _ in 0..len {
        let key = read_value(cursor, depth + 1)?;
        let val = read_value(cursor, depth + 1)?;
        pairs.push((key, val));
    }
    Ok(Value::Map(pairs))
}

fn read_bytes<'a>(cursor: &mut SliceCursor<'a>, len: usize) -> Result<&'a [u8], WireError> {
    if cursor.len() < len {
        return wire_err(format!(
            "unexpected end of MessagePack input (need {len} bytes, have {})",
            cursor.len()
        ));
    }
    let (head, tail) = cursor.split_at(len);
    *cursor = tail;
    Ok(head)
}

fn read_be(cursor: &mut SliceCursor, width: usize) -> Result<u64, WireError> {
    let bytes = read_bytes(cursor, width)?;
    let mut value = 0u64;
    for byte in bytes {
        value = (value << 8) | (*byte as u64);
    }
    Ok(value)
}

fn read_u8(cursor: &mut SliceCursor) -> Result<u8, WireError> {
    Ok(read_bytes(cursor, 1)?[0])
}

fn read_u16(cursor: &mut SliceCursor) -> Result<u16, WireError> {
    let bytes = read_bytes(cursor, 2)?;
    Ok(u16::from_be_bytes([bytes[0], bytes[1]]))
}

fn read_u32(cursor: &mut SliceCursor) -> Result<u32, WireError> {
    let bytes = read_bytes(cursor, 4)?;
    Ok(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn read_str_len_after_marker(cursor: &mut SliceCursor, marker: Marker) -> Result<u32, WireError> {
    match marker {
        Marker::Str8 => Ok(read_u8(cursor)? as u32),
        Marker::Str16 => Ok(read_u16(cursor)? as u32),
        Marker::Str32 => read_u32(cursor),
        _ => wire_err(format!("not a str marker: {marker:?}")),
    }
}

fn read_bin_len_after_marker(cursor: &mut SliceCursor, marker: Marker) -> Result<u32, WireError> {
    match marker {
        Marker::Bin8 => Ok(read_u8(cursor)? as u32),
        Marker::Bin16 => Ok(read_u16(cursor)? as u32),
        Marker::Bin32 => read_u32(cursor),
        _ => wire_err(format!("not a bin marker: {marker:?}")),
    }
}

fn read_array_len_after_marker(cursor: &mut SliceCursor, marker: Marker) -> Result<u32, WireError> {
    match marker {
        Marker::Array16 => Ok(read_u16(cursor)? as u32),
        Marker::Array32 => read_u32(cursor),
        _ => wire_err(format!("not an array marker: {marker:?}")),
    }
}

fn read_map_len_after_marker(cursor: &mut SliceCursor, marker: Marker) -> Result<u32, WireError> {
    match marker {
        Marker::Map16 => Ok(read_u16(cursor)? as u32),
        Marker::Map32 => read_u32(cursor),
        _ => wire_err(format!("not a map marker: {marker:?}")),
    }
}

impl Value {
    /// The value as `u64`; accepts any integer width and non-negative signed
    /// encodings.
    pub fn as_u64(&self) -> Result<u64, WireError> {
        match self {
            Value::UInt(v) => Ok(*v),
            Value::Int(v) if *v >= 0 => Ok(*v as u64),
            _ => wire_err(format!("expected an unsigned integer, found {self:?}")),
        }
    }

    /// The value as `i64`; accepts any integer width.
    pub fn as_i64(&self) -> Result<i64, WireError> {
        match self {
            Value::Int(v) => Ok(*v),
            Value::UInt(v) if *v <= i64::MAX as u64 => Ok(*v as i64),
            _ => wire_err(format!("expected a signed integer, found {self:?}")),
        }
    }

    /// The value as `bool`.
    pub fn as_bool(&self) -> Result<bool, WireError> {
        match self {
            Value::Bool(v) => Ok(*v),
            _ => wire_err(format!("expected a bool, found {self:?}")),
        }
    }

    /// The value as `f64`; `f32` markers are accepted and widened.
    pub fn as_f64(&self) -> Result<f64, WireError> {
        match self {
            Value::F64(v) => Ok(*v),
            Value::F32(v) => Ok(*v as f64),
            _ => wire_err(format!("expected a float, found {self:?}")),
        }
    }

    /// The value as `f32`; `f64` markers are accepted and narrowed.
    pub fn as_f32(&self) -> Result<f32, WireError> {
        match self {
            Value::F32(v) => Ok(*v),
            Value::F64(v) => Ok(*v as f32),
            _ => wire_err(format!("expected a float, found {self:?}")),
        }
    }

    /// The value as a string slice.
    pub fn as_str(&self) -> Result<&str, WireError> {
        match self {
            Value::Str(s) => Ok(s.as_str()),
            _ => wire_err(format!("expected a string, found {self:?}")),
        }
    }

    /// The value as an owned string.
    pub fn as_string(&self) -> Result<String, WireError> {
        Ok(self.as_str()?.to_string())
    }

    /// The value as a single character (a one-char string).
    pub fn as_char(&self) -> Result<char, WireError> {
        let s = self.as_str()?;
        let mut chars = s.chars();
        match (chars.next(), chars.next()) {
            (Some(c), None) => Ok(c),
            _ => wire_err(format!("expected a single-character string, found {s:?}")),
        }
    }

    /// The value as a byte string: the canonical bin form or, leniently, an
    /// array of `u8` integers.
    pub fn as_bin(&self) -> Result<Vec<u8>, WireError> {
        match self {
            Value::Bin(bytes) => Ok(bytes.clone()),
            Value::Array(items) => {
                let mut out = Vec::with_capacity(items.len());
                for item in items {
                    out.push(u8::from_value(item)?);
                }
                Ok(out)
            }
            _ => wire_err(format!("expected a byte string, found {self:?}")),
        }
    }

    /// The value as an array slice.
    pub fn as_array(&self) -> Result<&[Value], WireError> {
        match self {
            Value::Array(items) => Ok(items.as_slice()),
            _ => wire_err(format!("expected an array, found {self:?}")),
        }
    }

    /// The value as a map slice.
    pub fn as_map(&self) -> Result<&[(Value, Value)], WireError> {
        match self {
            Value::Map(pairs) => Ok(pairs.as_slice()),
            _ => wire_err(format!("expected a map, found {self:?}")),
        }
    }
}

/// Conversion into the wire [`Value`] model.
pub trait ToValue {
    /// Convert to the dynamic wire value.
    fn to_value(&self) -> Value;
}

/// Conversion from the wire [`Value`] model.
pub trait FromValue: Sized {
    /// Convert from the dynamic wire value.
    fn from_value(value: &Value) -> Result<Self, WireError>;
}

macro_rules! impl_value_unsigned {
    ($($ty:ty),+) => {
        $(
            impl ToValue for $ty {
                fn to_value(&self) -> Value {
                    Value::UInt(*self as u64)
                }
            }

            impl FromValue for $ty {
                fn from_value(value: &Value) -> Result<Self, WireError> {
                    let raw = value.as_u64()?;
                    <$ty>::try_from(raw).map_err(|_| {
                        WireError::new(format!(
                            "integer {raw} out of range for {}",
                            stringify!($ty)
                        ))
                    })
                }
            }

            impl From<$ty> for Value {
                fn from(v: $ty) -> Value {
                    Value::UInt(v as u64)
                }
            }
        )+
    };
}

macro_rules! impl_value_signed {
    ($($ty:ty),+) => {
        $(
            impl ToValue for $ty {
                fn to_value(&self) -> Value {
                    Value::Int(*self as i64)
                }
            }

            impl FromValue for $ty {
                fn from_value(value: &Value) -> Result<Self, WireError> {
                    let raw = value.as_i64()?;
                    <$ty>::try_from(raw).map_err(|_| {
                        WireError::new(format!(
                            "integer {raw} out of range for {}",
                            stringify!($ty)
                        ))
                    })
                }
            }

            impl From<$ty> for Value {
                fn from(v: $ty) -> Value {
                    Value::Int(v as i64)
                }
            }
        )+
    };
}

impl_value_unsigned!(u8, u16, u32, u64);
impl_value_signed!(i8, i16, i32, i64);

impl ToValue for bool {
    fn to_value(&self) -> Value {
        Value::Bool(*self)
    }
}

impl FromValue for bool {
    fn from_value(value: &Value) -> Result<Self, WireError> {
        value.as_bool()
    }
}

impl From<bool> for Value {
    fn from(v: bool) -> Value {
        Value::Bool(v)
    }
}

impl ToValue for f32 {
    fn to_value(&self) -> Value {
        Value::F32(*self)
    }
}

impl FromValue for f32 {
    fn from_value(value: &Value) -> Result<Self, WireError> {
        value.as_f32()
    }
}

impl From<f32> for Value {
    fn from(v: f32) -> Value {
        Value::F32(v)
    }
}

impl ToValue for f64 {
    fn to_value(&self) -> Value {
        Value::F64(*self)
    }
}

impl FromValue for f64 {
    fn from_value(value: &Value) -> Result<Self, WireError> {
        value.as_f64()
    }
}

impl From<f64> for Value {
    fn from(v: f64) -> Value {
        Value::F64(v)
    }
}

impl ToValue for char {
    fn to_value(&self) -> Value {
        Value::Str(self.to_string())
    }
}

impl FromValue for char {
    fn from_value(value: &Value) -> Result<Self, WireError> {
        value.as_char()
    }
}

impl ToValue for String {
    fn to_value(&self) -> Value {
        Value::Str(self.clone())
    }
}

impl ToValue for str {
    fn to_value(&self) -> Value {
        Value::Str(self.to_string())
    }
}

impl FromValue for String {
    fn from_value(value: &Value) -> Result<Self, WireError> {
        value.as_string()
    }
}

impl From<&str> for Value {
    fn from(v: &str) -> Value {
        Value::Str(v.to_string())
    }
}

/// `Vec<u8>` converts to an array of integers (the record-context form of
/// `list<u8>`), NOT a bin value; func-level blobs are encoded explicitly as
/// [`Value::Bin`] by the generated func codecs.
impl ToValue for Vec<u8> {
    fn to_value(&self) -> Value {
        to_array(self, |byte| Value::from(*byte))
    }
}

/// Accepts both the bin and the array-of-int form (see [`Value::as_bin`]).
impl FromValue for Vec<u8> {
    fn from_value(value: &Value) -> Result<Self, WireError> {
        value.as_bin()
    }
}

impl<T: ToValue> ToValue for Box<T> {
    fn to_value(&self) -> Value {
        (**self).to_value()
    }
}

impl<T: FromValue> FromValue for Box<T> {
    fn from_value(value: &Value) -> Result<Self, WireError> {
        T::from_value(value).map(Box::new)
    }
}

/// Encodes a slice as a MessagePack array value, mapping each element.
pub fn to_array<T>(items: &[T], f: impl Fn(&T) -> Value) -> Value {
    Value::Array(items.iter().map(f).collect())
}

/// Decodes a MessagePack array value, mapping each element.
pub fn from_array<T>(
    value: &Value,
    f: impl Fn(&Value) -> Result<T, WireError>,
) -> Result<Vec<T>, WireError> {
    let items = value.as_array()?;
    items.iter().map(f).collect()
}

/// Encodes an option: `None` is nil, `Some` is the mapped inner value.
pub fn to_option<T>(option: &Option<T>, f: impl Fn(&T) -> Value) -> Value {
    match option {
        Some(inner) => f(inner),
        None => Value::Nil,
    }
}

/// Decodes an option: nil is `None`, anything else maps to `Some`.
pub fn from_option<T>(
    value: &Value,
    f: impl Fn(&Value) -> Result<T, WireError>,
) -> Result<Option<T>, WireError> {
    match value {
        Value::Nil => Ok(None),
        other => Ok(Some(f(other)?)),
    }
}

/// Encodes a 2-tuple as a fixed 2-element array value.
pub fn to_tuple2<A, B>(
    tuple: &(A, B),
    fa: impl Fn(&A) -> Value,
    fb: impl Fn(&B) -> Value,
) -> Value {
    Value::Array(vec![fa(&tuple.0), fb(&tuple.1)])
}

/// Decodes a fixed 2-element array value into a 2-tuple.
pub fn from_tuple2<A, B>(
    value: &Value,
    fa: impl Fn(&Value) -> Result<A, WireError>,
    fb: impl Fn(&Value) -> Result<B, WireError>,
) -> Result<(A, B), WireError> {
    let items = value.as_array()?;
    if items.len() != 2 {
        return wire_err(format!(
            "expected a 2-element array, found {} elements",
            items.len()
        ));
    }
    Ok((fa(&items[0])?, fb(&items[1])?))
}

/// Encodes a 3-tuple as a fixed 3-element array value.
pub fn to_tuple3<A, B, C>(
    tuple: &(A, B, C),
    fa: impl Fn(&A) -> Value,
    fb: impl Fn(&B) -> Value,
    fc: impl Fn(&C) -> Value,
) -> Value {
    Value::Array(vec![fa(&tuple.0), fb(&tuple.1), fc(&tuple.2)])
}

/// Decodes a fixed 3-element array value into a 3-tuple.
pub fn from_tuple3<A, B, C>(
    value: &Value,
    fa: impl Fn(&Value) -> Result<A, WireError>,
    fb: impl Fn(&Value) -> Result<B, WireError>,
    fc: impl Fn(&Value) -> Result<C, WireError>,
) -> Result<(A, B, C), WireError> {
    let items = value.as_array()?;
    if items.len() != 3 {
        return wire_err(format!(
            "expected a 3-element array, found {} elements",
            items.len()
        ));
    }
    Ok((fa(&items[0])?, fb(&items[1])?, fc(&items[2])?))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_encoding(value: Value, expected: &[u8]) {
        assert_eq!(encode_value(&value), expected);
        assert_eq!(decode_value(expected).unwrap(), value);
    }

    #[test]
    fn integer_encodings_are_minimal_width() {
        assert_encoding(Value::UInt(0), &[0x00]);
        assert_encoding(Value::UInt(127), &[0x7f]);
        assert_encoding(Value::UInt(128), &[0xcc, 0x80]);
        assert_encoding(Value::UInt(255), &[0xcc, 0xff]);
        assert_encoding(Value::UInt(256), &[0xcd, 0x01, 0x00]);
        assert_encoding(Value::UInt(65536), &[0xce, 0x00, 0x01, 0x00, 0x00]);
        assert_encoding(
            Value::UInt(u64::from(u32::MAX) + 1),
            &[0xcf, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00],
        );
        assert_encoding(Value::Int(-1), &[0xff]);
        assert_encoding(Value::Int(-32), &[0xe0]);
        assert_encoding(Value::Int(-33), &[0xd0, 0xdf]);
        assert_encoding(Value::Int(-129), &[0xd1, 0xff, 0x7f]);
        // Non-negative signed values follow the unsigned chain and decode
        // back as unsigned.
        assert_eq!(encode_value(&Value::Int(127)), vec![0x7f]);
        assert_eq!(encode_value(&Value::Int(128)), vec![0xcc, 0x80]);
        assert_eq!(decode_value(&[0x7f]).unwrap(), Value::UInt(127));
    }

    #[test]
    fn string_and_bin_lengths_choose_markers() {
        assert_encoding(Value::Str("a".repeat(31)), &{
            let mut v = vec![0xbf];
            v.extend(std::iter::repeat_n(b'a', 31));
            v
        });
        assert_encoding(Value::Str("a".repeat(32)), &{
            let mut v = vec![0xd9, 32];
            v.extend(std::iter::repeat_n(b'a', 32));
            v
        });
        assert_encoding(Value::Bin(vec![0xab; 256]), &{
            let mut v = vec![0xc5, 0x01, 0x00];
            v.extend(std::iter::repeat_n(0xab, 256));
            v
        });
        assert_encoding(Value::Array(vec![]), &[0x90]);
        assert_encoding(Value::Map(vec![]), &[0x80]);
        assert_encoding(Value::Nil, &[0xc0]);
        assert_encoding(Value::Bool(true), &[0xc3]);
        assert_encoding(Value::Bool(false), &[0xc2]);
    }

    #[test]
    fn decode_accepts_any_integer_width() {
        // u16 marker carrying 5 decodes into a u8 target.
        assert_eq!(
            u8::from_value(&decode_value(&[0xcd, 0x00, 0x05]).unwrap()).unwrap(),
            5
        );
        // Signed non-negative marker into an unsigned target.
        assert_eq!(u64::from_value(&Value::Int(7)).unwrap(), 7);
        // Unsigned marker into a signed target.
        assert_eq!(i64::from_value(&Value::UInt(9)).unwrap(), 9);
        // Negative into unsigned is rejected.
        assert!(u64::from_value(&Value::Int(-1)).is_err());
        // Range narrowing is checked.
        assert!(u8::from_value(&Value::UInt(300)).is_err());
    }

    #[test]
    fn bin_accepts_array_form() {
        assert_eq!(
            Value::Array(vec![1u8.into(), 2u8.into()]).as_bin().unwrap(),
            vec![1, 2]
        );
        assert_eq!(Value::Bin(vec![3]).as_bin().unwrap(), vec![3]);
    }

    #[test]
    fn decode_rejects_trailing_bytes_and_garbage() {
        assert!(decode_value(&[0x00, 0x00]).is_err());
        assert!(decode_value(&[0xc1]).is_err());
        assert!(decode_value(&[0xa1, 0xff]).is_err());
        let (value, used) = decode_value_prefix(&[0x01, 0x02]).unwrap();
        assert_eq!(value, Value::UInt(1));
        assert_eq!(used, 1);
    }

    #[test]
    fn canonical_bytes_for_nested_structure() {
        // A nested structure exercising ints, strings, bin, arrays and maps.
        let value = Value::Map(vec![
            (Value::UInt(1), Value::Str("hello".to_string())),
            (
                Value::UInt(2),
                Value::Array(vec![Value::Int(-5), Value::F64(1.5), Value::Nil]),
            ),
            (Value::UInt(3), Value::Bin(vec![0xde, 0xad])),
        ]);
        let bytes = encode_value(&value);
        assert_eq!(
            bytes,
            vec![
                0x83, 0x01, 0xa5, b'h', b'e', b'l', b'l', b'o', 0x02, 0x93, 0xfb, 0xcb, 0x3f, 0xf8,
                0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xc0, 0x03, 0xc4, 0x02, 0xde, 0xad
            ]
        );
        assert_eq!(decode_value(&bytes).unwrap(), value);
    }

    #[test]
    fn encoder_matches_rmp_serde_byte_for_byte() {
        for v in [
            0u64,
            1,
            127,
            128,
            255,
            256,
            65535,
            65536,
            u32::MAX as u64,
            u32::MAX as u64 + 1,
            u64::MAX,
        ] {
            assert_eq!(
                encode_value(&Value::UInt(v)),
                rmp_serde::to_vec(&v).unwrap(),
                "u64 {v}"
            );
        }
        for v in [
            0i64,
            1,
            127,
            128,
            -1,
            -32,
            -33,
            -128,
            -129,
            -32768,
            -32769,
            i32::MIN as i64,
            i64::MIN,
            i64::MAX,
        ] {
            assert_eq!(
                encode_value(&Value::Int(v)),
                rmp_serde::to_vec(&v).unwrap(),
                "i64 {v}"
            );
        }
        for s in [
            String::new(),
            "a".repeat(31),
            "a".repeat(32),
            "a".repeat(255),
            "a".repeat(256),
            "héllo世界".to_string(),
        ] {
            assert_eq!(
                encode_value(&Value::Str(s.clone())),
                rmp_serde::to_vec(&s).unwrap(),
                "str len {}",
                s.len()
            );
        }
        assert_eq!(
            encode_value(&Value::F32(0.1)),
            rmp_serde::to_vec(&0.1f32).unwrap()
        );
        assert_eq!(
            encode_value(&Value::F64(-2.5)),
            rmp_serde::to_vec(&-2.5f64).unwrap()
        );
        assert_eq!(
            encode_value(&Value::Nil),
            rmp_serde::to_vec(&Option::<u8>::None).unwrap()
        );
        for len in [0usize, 15, 16] {
            let arr = vec![0u64; len];
            assert_eq!(
                encode_value(&to_array(&arr, |x| Value::from(*x))),
                rmp_serde::to_vec(&arr).unwrap(),
                "array len {len}"
            );
        }
        let map: std::collections::BTreeMap<u8, u8> = [(1, 2), (3, 4)].into_iter().collect();
        assert_eq!(
            encode_value(&Value::Map(vec![
                (1u8.into(), 2u8.into()),
                (3u8.into(), 4u8.into())
            ])),
            rmp_serde::to_vec(&map).unwrap()
        );
    }
}
