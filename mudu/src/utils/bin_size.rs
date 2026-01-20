use crate::common::buf::Buf;
use crate::common::endian::Endian;
use byteorder::ByteOrder;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BinSize {
    len: u32,
}

impl BinSize {
    pub fn from_slice(slice: &[u8]) -> Self {
        if slice.len() < Self::size_of() {
            panic!("binary size capacity  error");
        }
        let len = Endian::read_u32(slice);
        Self::new(len)
    }

    pub fn copy_to_slice(&self, binary: &mut [u8]) {
        if binary.len() < Self::size_of() {
            panic!("binary length capacity  error");
        }
        Endian::write_u32(binary, self.len);
    }

    pub fn to_binary(&self) -> Buf {
        let mut buf: Buf = vec![0; Self::size_of()];
        Endian::write_u32(&mut buf, self.len);
        buf
    }

    pub fn new(len: u32) -> Self {
        Self { len }
    }

    pub fn size(&self) -> u32 {
        self.len
    }

    pub fn size_of() -> usize {
        size_of::<u32>()
    }
}
