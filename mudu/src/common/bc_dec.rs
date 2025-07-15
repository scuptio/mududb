use crate::common::bc::{BCHdr, BCTail};
use crate::common::endian::Endian;
use crate::common::slice::SliceRef;
use byteorder::ByteOrder;
use std::fmt::{Display, Formatter};

#[derive(Debug, Clone)]
pub enum DecErr {
    CapacityNotAvailable,
    EmptyEnum { type_name: String },
    ErrorCRC,
}

impl Display for DecErr {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)?;
        Ok(())
    }
}

pub trait Decoder {
    fn read_i8(&mut self, _n: u8) -> Result<i8, DecErr> {
        let mut s = [0i8; 1];
        let bytes = unsafe { &mut *(&mut s as *mut [i8] as *mut [u8]) };
        self.read(bytes)?;
        Ok(s[0])
    }

    fn read_u8(&mut self) -> Result<u8, DecErr> {
        let mut s = [0u8; 1];
        self.read(&mut s)?;
        Ok(s[0])
    }

    fn read_u32(&mut self) -> Result<u32, DecErr> {
        let mut s = [0u8; 4];
        self.read(&mut s)?;
        let n = Endian::read_u32(&s);
        Ok(n)
    }

    fn read_i32(&mut self) -> Result<i32, DecErr> {
        let mut s = [0u8; 4];
        self.read(&mut s)?;
        let n = Endian::read_i32(&s);
        Ok(n)
    }

    fn read_i64(&mut self) -> Result<i64, DecErr> {
        let mut s = [0u8; 8];
        self.read(&mut s)?;
        let n = Endian::read_i64(&s);
        Ok(n)
    }

    fn read_u64(&mut self) -> Result<u64, DecErr> {
        let mut s = [0u8; 8];
        self.read(&mut s)?;
        let n = Endian::read_u64(&s);
        Ok(n)
    }

    fn read_i128(&mut self) -> Result<i128, DecErr> {
        let mut s = [0u8; 16];
        self.read(&mut s)?;
        let n = Endian::read_i128(&s);
        Ok(n)
    }

    fn read_u128(&mut self) -> Result<u128, DecErr> {
        let mut s = [0u8; 16];
        self.read(&mut s)?;
        let n = Endian::read_u128(&s);
        Ok(n)
    }

    fn read_bytes(&mut self, s: &mut [u8]) -> Result<(), DecErr> {
        self.read(s)?;
        Ok(())
    }

    fn read(&mut self, s: &mut [u8]) -> Result<(), DecErr>;
}

pub trait Decode: Sized {
    /// Encode a given type.
    fn decode<D: Decoder>(decoder: &mut D) -> Result<Self, DecErr>;
}

fn decode_binary_header(slice: &[u8]) -> Result<(u32, u64), DecErr> {
    let mut s = SliceRef::new(slice);
    let hdr = BCHdr::decode(&mut s)?;
    Ok((hdr.length(), hdr.crc()))
}

fn decode_binary_tail(slice: &[u8]) -> Result<u64, DecErr> {
    let mut s = SliceRef::new(slice);
    let tail = BCTail::decode(&mut s)?;
    Ok(tail.crc())
}

fn decode_binary_body<D: Decode>(slice: &[u8]) -> Result<D, DecErr> {
    let mut r = SliceRef::new(slice);
    let d = D::decode(&mut r)?;
    Ok(d)
}

pub fn decode_binary<D: Decode>(slice: &[u8]) -> Result<D, DecErr> {
    if slice.len() < BCHdr::hdr_size() + BCTail::tail_size() {
        return Err(DecErr::CapacityNotAvailable);
    }
    let (length, start_crc) = decode_binary_header(&slice[0..BCHdr::hdr_size()])?;
    let d =
        decode_binary_body::<D>(&slice[BCHdr::hdr_size()..BCHdr::hdr_size() + length as usize])?;
    let end_crc = decode_binary_tail(&slice[BCHdr::hdr_size() + length as usize..])?;
    if start_crc != end_crc {
        return Err(DecErr::ErrorCRC);
    }
    Ok(d)
}
