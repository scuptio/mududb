use crate::common::bc_dec::{DecErr, Decoder};
use crate::common::bc_enc::{EncErr, Encoder};
pub struct SliceRef<'r> {
    s: &'r [u8],
    read_pos: usize,
}

pub struct SliceMutRef<'r> {
    s: &'r mut [u8],
    write_pos: usize,
}

impl<'r> SliceMutRef<'r> {
    pub fn new(s: &'r mut [u8]) -> Self {
        Self { s, write_pos: 0 }
    }
    pub fn capacity(&self) -> usize {
        self.s.len()
    }

    pub fn write_pos(&self) -> usize {
        self.write_pos
    }

    pub fn set_write_pos(&mut self) {
        self.write_pos = 0;
    }

    pub fn as_slice(&self) -> &[u8] {
        &self.s[..self.write_pos]
    }
}

impl<'r> SliceRef<'r> {
    pub fn new(s: &'r [u8]) -> Self {
        Self { s, read_pos: 0 }
    }

    pub fn read_pos(&self) -> usize {
        self.read_pos
    }

    pub fn resize(&mut self) {
        self.read_pos = 0;
    }

    pub fn as_slice(&self) -> &'r [u8] {
        &self.s[..self.read_pos]
    }
}

impl Encoder for SliceMutRef<'_> {
    fn write(&mut self, bytes: &[u8]) -> Result<(), EncErr> {
        if self.s.len() >= self.write_pos + bytes.len() {
            self.s[self.write_pos..self.write_pos + bytes.len()].copy_from_slice(bytes);
            self.write_pos += bytes.len();
            Ok(())
        } else {
            Err(EncErr::CapacityNotAvailable)
        }
    }
}

impl Decoder for SliceRef<'_> {
    fn read(&mut self, bytes: &mut [u8]) -> Result<(), DecErr> {
        if self.s.len() >= self.read_pos + bytes.len() {
            bytes.copy_from_slice(&self.s[self.read_pos..self.read_pos + bytes.len()]);
            self.read_pos += bytes.len();
            Ok(())
        } else {
            Err(DecErr::CapacityNotAvailable)
        }
    }
}
