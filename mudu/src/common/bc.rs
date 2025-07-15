use crate::common::bc_dec::{DecErr, Decode, Decoder};
use crate::common::bc_enc::{EncErr, Encode, Encoder};
use std::mem::size_of;

pub fn hdr_size() -> usize {
    BCHdr::hdr_size()
}

pub fn tail_size() -> usize {
    BCTail::tail_size()
}

/// header,
///     4 bytes body size
///     8 bytes body crc
pub struct BCHdr {
    length: u32,
    crc: u64,
}

/// tail,
///     8 bytes body crc
pub struct BCTail {
    crc: u64,
}

impl BCHdr {
    pub fn new(length: u32, crc: u64) -> Self {
        Self { length, crc }
    }

    // body length
    pub fn length(&self) -> u32 {
        self.length
    }

    pub fn crc(&self) -> u64 {
        self.crc
    }

    pub fn hdr_size() -> usize {
        // length size +  crc size
        size_of::<u32>() + size_of::<u64>()
    }
}

impl Decode for BCHdr {
    fn decode<D: Decoder>(decoder: &mut D) -> Result<Self, DecErr> {
        let length = decoder.read_u32()?;
        let crc = decoder.read_u64()?;
        Ok(Self { length, crc })
    }
}

impl Encode for BCHdr {
    fn encode<E: Encoder>(&self, encoder: &mut E) -> Result<(), EncErr> {
        encoder.write_u32(self.length)?;
        encoder.write_u64(self.crc)?;
        Ok(())
    }

    fn size(&self) -> Result<usize, EncErr> {
        Ok(size_of::<u32>() + size_of::<u64>())
    }
}

impl BCTail {
    pub fn new(crc: u64) -> Self {
        Self { crc }
    }

    pub fn crc(&self) -> u64 {
        self.crc
    }

    pub fn tail_size() -> usize {
        size_of::<u64>()
    }
}

impl Decode for BCTail {
    fn decode<D: Decoder>(decoder: &mut D) -> Result<Self, DecErr> {
        let crc = decoder.read_u64()?;
        Ok(Self { crc })
    }
}

impl Encode for BCTail {
    fn encode<E: Encoder>(&self, encoder: &mut E) -> Result<(), EncErr> {
        encoder.write_u64(self.crc)?;
        Ok(())
    }

    fn size(&self) -> Result<usize, EncErr> {
        Ok(Self::tail_size())
    }
}
