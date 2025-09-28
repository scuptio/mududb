#[cfg(any(test, feature = "test"))]
use arbitrary::Arbitrary;
use mudu::common::bc_dec::{DecErr, Decode, Decoder};
use mudu::common::bc_enc::{EncErr, Encode, Encoder};

#[cfg_attr(any(test, feature = "test"), derive(Arbitrary))]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Timestamp {
    c_min: u64,
    c_max: u64,
}

impl Timestamp {
    pub fn new(c_min: u64, c_max: u64) -> Self {
        Self { c_min, c_max }
    }

    pub fn c_max(&self) -> u64 {
        self.c_max
    }

    pub fn c_min(&self) -> u64 {
        self.c_min
    }

    pub fn size_of() -> usize {
        size_of::<u64>() + size_of::<u64>()
    }
}

impl Decode for Timestamp {
    fn decode<D: Decoder>(decoder: &mut D) -> Result<Self, DecErr> {
        let c_min = decoder.read_u64()?;
        let c_max = decoder.read_u64()?;
        Ok(Self::new(c_min, c_max))
    }
}

impl Encode for Timestamp {
    fn encode<E: Encoder>(&self, encoder: &mut E) -> Result<(), EncErr> {
        encoder.write_u64(self.c_min)?;
        encoder.write_u64(self.c_max)?;
        Ok(())
    }

    fn size(&self) -> Result<usize, EncErr> {
        Ok(Self::size_of())
    }
}

impl Default for Timestamp {
    fn default() -> Self {
        Self {
            c_min: 0,
            c_max: u64::MAX,
        }
    }
}
