use crate::common::buf::Buf;
use crate::common::endian::Endian;
use byteorder::ByteOrder;
use std::error::Error;
use std::fmt::{Display, Formatter};

#[derive(Debug, Clone)]
pub enum DecErr {
    CapacityNotAvailable,
    EmptyEnum { type_name: String },
    ErrorCRC,
}

impl Error for DecErr {}

impl Display for DecErr {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[derive(Debug, Ord, PartialOrd, PartialEq, Eq)]
pub enum EncErr {
    CapacityNotAvailable,
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
        Ok(Endian::read_u32(&s))
    }

    fn read_i32(&mut self) -> Result<i32, DecErr> {
        let mut s = [0u8; 4];
        self.read(&mut s)?;
        Ok(Endian::read_i32(&s))
    }

    fn read_i64(&mut self) -> Result<i64, DecErr> {
        let mut s = [0u8; 8];
        self.read(&mut s)?;
        Ok(Endian::read_i64(&s))
    }

    fn read_u64(&mut self) -> Result<u64, DecErr> {
        let mut s = [0u8; 8];
        self.read(&mut s)?;
        Ok(Endian::read_u64(&s))
    }

    fn read_i128(&mut self) -> Result<i128, DecErr> {
        let mut s = [0u8; 16];
        self.read(&mut s)?;
        Ok(Endian::read_i128(&s))
    }

    fn read_u128(&mut self) -> Result<u128, DecErr> {
        let mut s = [0u8; 16];
        self.read(&mut s)?;
        Ok(Endian::read_u128(&s))
    }

    fn read_bytes(&mut self, s: &mut [u8]) -> Result<(), DecErr> {
        self.read(s)
    }

    fn read(&mut self, s: &mut [u8]) -> Result<(), DecErr>;
}

pub trait Decode: Sized {
    fn decode<D: Decoder>(decoder: &mut D) -> Result<Self, DecErr>;
}

pub trait Encoder {
    fn write_i8(&mut self, n: i8) -> Result<(), EncErr> {
        let a = [n];
        let bytes = unsafe { &*(&a as *const [i8] as *const [u8]) };
        self.write(bytes)
    }

    fn write_u8(&mut self, n: u8) -> Result<(), EncErr> {
        self.write(&[n])
    }

    fn write_i32(&mut self, n: i32) -> Result<(), EncErr> {
        let mut buf = [0; 4];
        Endian::write_i32(&mut buf, n);
        self.write(&buf)
    }

    fn write_u32(&mut self, n: u32) -> Result<(), EncErr> {
        let mut buf = [0; 4];
        Endian::write_u32(&mut buf, n);
        self.write(&buf)
    }

    fn write_i64(&mut self, n: i64) -> Result<(), EncErr> {
        let mut buf = [0; 8];
        Endian::write_i64(&mut buf, n);
        self.write(&buf)
    }

    fn write_u64(&mut self, n: u64) -> Result<(), EncErr> {
        let mut buf = [0; 8];
        Endian::write_u64(&mut buf, n);
        self.write(&buf)
    }

    fn write_i128(&mut self, n: i128) -> Result<(), EncErr> {
        let mut buf = [0; 16];
        Endian::write_i128(&mut buf, n);
        self.write(&buf)
    }

    fn write_u128(&mut self, n: u128) -> Result<(), EncErr> {
        let mut buf = [0; 16];
        Endian::write_u128(&mut buf, n);
        self.write(&buf)
    }

    fn write_bytes(&mut self, s: &[u8]) -> Result<(), EncErr> {
        self.write(s)
    }

    fn write(&mut self, s: &[u8]) -> Result<(), EncErr>;
}

pub trait Encode {
    fn encode<E: Encoder>(&self, encoder: &mut E) -> Result<(), EncErr>;

    fn size(&self) -> Result<usize, EncErr>;
}

impl Encoder for Buf {
    fn write(&mut self, bytes: &[u8]) -> Result<(), EncErr> {
        self.extend(bytes);
        Ok(())
    }
}

impl Decoder for (Buf, usize) {
    fn read(&mut self, bytes: &mut [u8]) -> Result<(), DecErr> {
        if self.0.len() >= self.1 + bytes.len() {
            bytes.copy_from_slice(&self.0[self.1..self.1 + bytes.len()]);
            self.1 += bytes.len();
            Ok(())
        } else {
            Err(DecErr::CapacityNotAvailable)
        }
    }
}
