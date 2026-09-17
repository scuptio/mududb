//! `tuple::slot` module.
#![allow(missing_docs)]

use byteorder::ByteOrder;
use mudu::common::buf::Buf;
use mudu::common::endian::Endian;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Slot {
    off: u32,
    len: u32,
}

impl Slot {
    pub fn from_binary(binary: &[u8]) -> RS<Self> {
        if binary.len() < Self::size_of() {
            return Err(mudu_error!(
                ErrorCode::Decode,
                format!(
                    "slot binary size {} is less than {}",
                    binary.len(),
                    Self::size_of()
                )
            ));
        }
        let off = Endian::read_u32(binary);
        let len = Endian::read_u32(&binary[size_of::<u32>()..]);
        Ok(Self::new(off, len))
    }

    pub fn to_binary(&self, binary: &mut [u8]) -> RS<()> {
        if binary.len() < Self::size_of() {
            return Err(mudu_error!(
                ErrorCode::Encode,
                format!(
                    "slot binary capacity {} is less than {}",
                    binary.len(),
                    Self::size_of()
                )
            ));
        }
        Endian::write_u32(binary, self.off);
        Endian::write_u32(&mut binary[size_of::<u32>()..], self.len);
        Ok(())
    }

    pub fn to_binary_buf(&self) -> RS<Buf> {
        let mut buf: Buf = vec![0; Self::size_of()];
        Endian::write_u32(&mut buf, self.off);
        Endian::write_u32(&mut buf[size_of::<u32>()..], self.len);
        Ok(buf)
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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
#[allow(clippy::expect_used)]
#[allow(clippy::panic)]
mod tests {
    use super::Slot;
    use mudu::error::ErrorCode;

    #[test]
    fn slot_rejects_short_binary() {
        let err = Slot::from_binary(&[0u8; 4]).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::Decode);
    }

    #[test]
    fn slot_rejects_short_target_buffer() {
        let slot = Slot::new(1, 2);
        let mut buf = [0u8; 4];
        let err = slot.to_binary(&mut buf).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::Encode);
    }
}
