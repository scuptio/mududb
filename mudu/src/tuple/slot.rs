use crate::common::buf::Buf;
use crate::common::endian::Endian;
use byteorder::ByteOrder;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug,
    Serialize,
    Deserialize,
)]
pub struct Slot {
    off: u32,
    len: u32,
}

impl Slot {
    pub fn from_binary(binary: &[u8]) -> Self {
        if binary.len() < Self::size_of() {
            panic!("slot capacity  error");
        }
        let off = Endian::read_u32(binary);
        let len = Endian::read_u32(&binary[size_of::<u32>()..]);
        Self::new(off, len)
    }

    pub fn to_binary(&self, binary: &mut [u8]) {
        if binary.len() < Self::size_of() {
            panic!("slot capacity  error");
        }
        Endian::write_u32(binary, self.off);
        Endian::write_u32(&mut binary[size_of::<u32>()..], self.len);
    }

    pub fn to_binary_buf(&self) -> Buf {
        let mut buf: Buf = vec![0; Self::size_of()];
        if buf.len() < Self::size_of() {
            panic!("slot capacity  error");
        }
        Endian::write_u32(&mut buf, self.off);
        Endian::write_u32(&mut buf[size_of::<u32>()..], self.len);
        buf
    }

    pub fn new(off: u32, len: u32) -> Self {
        Self { off, len }
    }

    pub fn offset(&self) -> usize {
        self.off as usize
    }

    pub fn length(&self) -> usize {
        self.len as usize
    }

    pub fn size_of() -> usize {
        size_of::<u32>() + size_of::<u32>()
    }
}
