use crate::common::endian;
use crate::common::result::RS;
use crate::error::ec::EC;
use crate::m_error;
use rmp_serde::{encode, Serializer as RmpSerializer};
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json;
use std::io;
use std::io::Write;

pub const fn header_size_len() -> u64 {
    SIZE_LEN as u64
}

pub fn deserialize_sized_from<D: DeserializeOwned + 'static>(deserialize: &[u8]) -> RS<(D, u64)> {
    _deserialize_sized_from(deserialize)
}

pub fn deserialize_from_json<S: DeserializeOwned>(json: &str) -> RS<S> {
    _deserialize_from_json(json)
}

pub fn serialize_sized_to<S: Serialize>(serialize: &S, out_buf: &mut [u8]) -> RS<(bool, u64)> {
    _serialize_sized_to(serialize, out_buf)
}

pub fn serialize_sized_to_vec<S: Serialize>(serialize: &S) -> RS<Vec<u8>> {
    _serialize_sized_to_vec(serialize)
}

const SIZE_LEN: usize = size_of::<u64>();

pub struct Writer<'a> {
    inner: &'a mut [u8],
    position: usize,
}

pub struct Sizer {
    size: usize,
}

impl<'a> Writer<'a> {
    fn new(inner: &'a mut [u8]) -> Self {
        Writer { inner, position: 0 }
    }

    fn written(&self) -> usize {
        self.position
    }

    fn remaining(&self) -> usize {
        self.inner.len() - self.position
    }
}

impl Sizer {
    fn new() -> Self {
        Self { size: 0 }
    }

    fn size(&self) -> usize {
        self.size
    }
}

impl<'a> Write for Writer<'a> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let remaining = self.remaining();
        let write_size = buf.len().min(remaining);

        if write_size > 0 {
            let end = self.position + write_size;
            self.inner[self.position..end].copy_from_slice(&buf[..write_size]);
            self.position += write_size;
        }

        Ok(write_size)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Write for Sizer {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.size += buf.len();
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub fn serialize_to_json<S: Serialize>(serialize: &S) -> RS<String> {
    let string = serde_json::to_string_pretty(serialize)
        .map_err(|e| m_error!(EC::EncodeErr, "serialize to json string error", e))?;
    Ok(string)
}

fn _deserialize_from_json<S: DeserializeOwned>(json: &str) -> RS<S> {
    let s = serde_json::from_str::<S>(json)
        .map_err(|e| m_error!(EC::DecodeErr, "deserialize json from string error", e))?;
    Ok(s)
}

fn _serialize_sized_to_vec<S: Serialize>(serialize: &S) -> RS<Vec<u8>> {
    let mut vec = Vec::<u8>::new();
    vec.resize(256, 0);
    let (ok, n) = _serialize_sized_to(serialize, &mut vec)?;
    if ok {
        vec.resize(n as usize + SIZE_LEN, 0);
    } else {
        vec.resize(n as usize + SIZE_LEN, 0);
        let (ok2, _) = _serialize_sized_to(serialize, &mut vec)?;
        if !ok2 {
            return Err(m_error!(
                EC::InsufficientBufferSpace,
                "insufficient buffer size to fill body"
            ));
        }
    }
    Ok(vec)
}

fn _deserialize_sized_from<D: DeserializeOwned + 'static>(input: &[u8]) -> RS<(D, u64)> {
    if input.len() < SIZE_LEN {
        return Err(m_error!(
            EC::InsufficientBufferSpace,
            "insufficient buffer size to fill length"
        ));
    }
    let length = decode_length(input);
    if length as usize + SIZE_LEN > input.len() {
        return Err(m_error!(
            EC::InsufficientBufferSpace,
            "insufficient buffer size to fill body"
        ));
    }
    let input_d: D = rmp_serde::decode::from_slice(&input[SIZE_LEN..SIZE_LEN + length as usize])
        .map_err(|e| m_error!(EC::DecodeErr, "decode error", e))?;

    Ok((input_d, length as _))
}

fn _serialize_sized_to<S: Serialize>(result: &S, out_buf: &mut [u8]) -> RS<(bool, u64)> {
    if out_buf.len() < SIZE_LEN {
        return Err(m_error!(
            EC::InsufficientBufferSpace,
            "insufficient buffer size to fill length"
        ));
    }
    let mut writer = Writer::new(&mut out_buf[SIZE_LEN..]);
    let mut serializer = RmpSerializer::new(&mut writer);
    let r = result.serialize(&mut serializer);
    match r {
        Ok(()) => {
            let size = writer.written() as u64;
            encode_length(out_buf, size);
            Ok((true, size as _))
        }
        Err(err) => {
            match &err {
                encode::Error::InvalidValueWrite(_err) => {
                    // it is possible that the buffer is not insufficient
                    let mut sizer = Sizer::new();
                    let mut serializer = RmpSerializer::new(&mut sizer);
                    let r = result.serialize(&mut serializer);
                    if r.is_err() {
                        let size = sizer.size() as u64;
                        encode_length(out_buf, size);
                        if size > out_buf.len() as u64 {
                            // the expected size > output buffer size
                            return Ok((true, size));
                        }
                    }
                    Err(m_error!(EC::EncodeErr, "serialize error", err))
                }
                _ => {
                    encode_length(out_buf, 0);
                    Err(m_error!(EC::EncodeErr, "encode error", err))
                }
            }
        }
    }
}

fn encode_length(out_buf: &mut [u8], size: u64) {
    endian::write_u64(&mut out_buf[0..SIZE_LEN], size);
}

fn decode_length(buf: &[u8]) -> u64 {
    endian::read_u64(buf)
}
