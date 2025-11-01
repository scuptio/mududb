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
const INIT_LENGTH: usize = 256usize;

pub fn deserialize_sized_from<D: Serialize + DeserializeOwned + 'static>(deserialize: &[u8]) -> RS<(D, u64)> {
    __deserialize_sized_from::<D, false>(deserialize)
}

pub fn serialize_sized_to<S: Serialize + DeserializeOwned + 'static>(serialize: &S, out_buf: &mut [u8]) -> RS<(bool, u64)> {
    __serialize_sized_to::<S, false>(serialize, out_buf)
}

pub fn serialize_sized_to_vec<S: Serialize + DeserializeOwned + 'static>(serialize: &S) -> RS<Vec<u8>> {
    __serialize_sized_to_vec::<S, false>(serialize)
}

pub fn deserialize_from_json<S: DeserializeOwned>(json: &str) -> RS<S> {
    _deserialize_from_json(json)
}

fn __deserialize_sized_from<D: Serialize + DeserializeOwned + 'static, const TEST: bool>(deserialize: &[u8]) -> RS<(D, u64)> {
    if TEST
    {
        let length = decode_length(deserialize);
        let arch = std::env::consts::ARCH;
        println!("{}: deserialize_sized_from, {} bytes:------------\n{:?}", arch, length, &deserialize[..length as usize + SIZE_LEN]);
    }
    let (d, size) = _deserialize_sized_from::<D>(deserialize)?;
    if TEST
    {
        let arch = std::env::consts::ARCH;
        println!("{}: deserialize_sized_from, json----\n{}", arch, serde_json::to_string(&d).unwrap());
        let _r = _serialize_sized_to_vec(&d);
        match _r {
            Ok(_vec) => {
                println!("{}: deserialize_sized_from, {} bytes test:------------\n{:?}", arch, _vec.len() - SIZE_LEN, _vec);
            }
            Err(_e) => { println!("{}: deserialize_sized_from, error {}", arch, serde_json::to_string(&_e).unwrap()) }
        }
    }
    Ok((d, size))
}


fn __serialize_sized_to<S: Serialize + DeserializeOwned + 'static, const TEST: bool>(serialize: &S, out_buf: &mut [u8]) -> RS<(bool, u64)> {
    let (ok, size) = _serialize_sized_to(serialize, out_buf)?;
    if TEST {
        __debug_serialize_helper(serialize, out_buf, size, "serialize_sized_to")
    }
    Ok((ok, size))
}

fn __debug_serialize_helper<S: Serialize + DeserializeOwned + 'static>(
    serialize: &S, out_buf: &[u8], size: u64, name: &str) {
    let target = std::env::consts::ARCH;
    println!("{}:{}, json ----\n{}", target, name, serde_json::to_string(serialize).unwrap());
    let _out = &out_buf[..size as usize + SIZE_LEN];
    println!("{}:{}, {} bytes:------------\n{:?}", target, name, size, _out);
    let _r = _deserialize_sized_from::<S>(_out);
    match _r {
        Ok((_s, _)) => {
            let _r_vec = _serialize_sized_to_vec(&_s);
            match _r_vec {
                Ok(_vec) => { println!("{}:{}, {} bytes test:------------\n{:?}", target, name, _vec.len() - SIZE_LEN, _vec); }
                Err(_e) => { println!("{}: {}, error {}", target, name, serde_json::to_string(&_e).unwrap()) }
            }
        }
        Err(_e) => { println!("{}: {}, error {}", target, name, serde_json::to_string(&_e).unwrap()) }
    };
}

pub fn __serialize_sized_to_vec<S: Serialize + DeserializeOwned + 'static, const TEST: bool>(serialize: &S) -> RS<Vec<u8>> {
    let vec = _serialize_sized_to_vec(serialize)?;
    if TEST {
        __debug_serialize_helper(serialize, &vec, vec.len() as u64 - SIZE_LEN as u64, "serialize_sized_to_vec")
    }
    Ok(vec)
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

fn _serialize_sized_to_vec<S: Serialize + DeserializeOwned + 'static>(serialize: &S) -> RS<Vec<u8>> {
    let mut vec = Vec::<u8>::new();
    vec.resize(INIT_LENGTH, 0);
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
        .map_err(|e| m_error!(EC::DecodeErr, format!("decode error {} bytes", length), e))?;

    Ok((input_d, length as _))
}

fn _serialize_sized_to<S: Serialize + DeserializeOwned + 'static>(result: &S, out_buf: &mut [u8]) -> RS<(bool, u64)> {
    if out_buf.len() < SIZE_LEN {
        return Err(m_error!(
            EC::InsufficientBufferSpace,
            "insufficient buffer size to fill length"
        ));
    }

    let out_max_len = out_buf.len() - SIZE_LEN;
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
                    if r.is_ok() {
                        let size = sizer.size() as u64;
                        encode_length(out_buf, size);
                        if size > out_max_len as u64 {
                            // the expected size > output buffer size
                            return Ok((false, size));
                        }
                        println!("size {}, max_buf_size {}", size, out_max_len);
                    } else {
                        return Err(m_error!(EC::EncodeErr, "serialize error", err));
                    }
                    println!("{}", serde_json::to_string_pretty(&result).unwrap());
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

#[cfg(test)]
mod tests {
    use crate::common::serde_utils::{__deserialize_sized_from, __serialize_sized_to_vec};
    use crate::database::v2h_param::CommandIn;
    use crate::tuple::datum_desc::DatumDesc;

    #[test]
    fn test_in_command() {
        let json = include_str!("test_data/in_command.json");
        let command_in = serde_json::from_str::<CommandIn>(json).unwrap();
        let vec = __serialize_sized_to_vec::<_, true>(&command_in).unwrap();
        let (command_in_1, _n) = __deserialize_sized_from::<CommandIn, true>(vec.as_slice()).unwrap();
        println!("{:?}", serde_json::to_string_pretty(&command_in_1).unwrap());
    }

    #[test]
    fn test_datum_desc() {
        let json = include_str!("test_data/datum_desc.json");
        let command_in = serde_json::from_str::<DatumDesc>(json).unwrap();
        let vec = __serialize_sized_to_vec::<_, true>(&command_in).unwrap();
        let (command_in_1, _n) = __deserialize_sized_from::<DatumDesc, true>(vec.as_slice()).unwrap();
        println!("{:?}", serde_json::to_string_pretty(&command_in_1).unwrap());
    }
}