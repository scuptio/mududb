//! `tuple::bitmap` module.
#![allow(missing_docs)]

use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bitmap {
    bits: usize,
    bytes: Vec<u8>,
}

impl Bitmap {
    pub fn new(bits: usize) -> Self {
        Self {
            bits,
            bytes: vec![0; aligned_byte_len(bits)],
        }
    }

    pub fn from_bytes(bits: usize, bytes: &[u8]) -> RS<Self> {
        let expected = aligned_byte_len(bits);
        if bytes.len() < expected {
            return Err(mudu_error!(
                ErrorCode::Decode,
                format!(
                    "bitmap requires {} bytes for {} bits, got {}",
                    expected,
                    bits,
                    bytes.len()
                )
            ));
        }
        Ok(Self {
            bits,
            bytes: bytes[..expected].to_vec(),
        })
    }

    pub fn get(&self, bit_idx: usize) -> RS<bool> {
        self.check_index(bit_idx)?;
        Ok((self.bytes[bit_idx / 8] & (1u8 << (bit_idx % 8))) != 0)
    }

    pub fn set(&mut self, bit_idx: usize, value: bool) -> RS<()> {
        self.check_index(bit_idx)?;
        let mask = 1u8 << (bit_idx % 8);
        if value {
            self.bytes[bit_idx / 8] |= mask;
        } else {
            self.bytes[bit_idx / 8] &= !mask;
        }
        Ok(())
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn bits(&self) -> usize {
        self.bits
    }

    pub fn byte_len(&self) -> usize {
        self.bytes.len()
    }

    fn check_index(&self, bit_idx: usize) -> RS<()> {
        if bit_idx >= self.bits {
            return Err(mudu_error!(
                ErrorCode::IndexOutOfRange,
                format!("bitmap bit index {} out of {}", bit_idx, self.bits)
            ));
        }
        Ok(())
    }
}

pub fn aligned_byte_len(bits: usize) -> usize {
    let bytes = bits.div_ceil(8);
    align_up(bytes, 8)
}

fn align_up(value: usize, align: usize) -> usize {
    if value == 0 {
        0
    } else {
        value.div_ceil(align) * align
    }
}
