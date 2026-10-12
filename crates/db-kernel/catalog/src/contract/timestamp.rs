#[cfg(any(test, feature = "test"))]
use arbitrary::Arbitrary;
use mudu::common::codec::{DecErr, Decode, Decoder, EncErr, Encode, Encoder};

/// Commit timestamp range of a tuple version: `c_min` is the creating
/// transaction, `c_max` the deleting one (`u64::MAX` while the version is
/// live).
#[cfg_attr(any(test, feature = "test"), derive(Arbitrary))]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Timestamp {
    c_min: u64,
    c_max: u64,
}

impl Timestamp {
    /// Creates a timestamp from the creator (`c_min`) and deleter (`c_max`)
    /// timestamps.
    pub fn new(c_min: u64, c_max: u64) -> Self {
        Self { c_min, c_max }
    }

    /// The deleting transaction's timestamp; `u64::MAX` while the version is
    /// live.
    pub fn c_max(&self) -> u64 {
        self.c_max
    }

    /// The creating transaction's timestamp.
    pub fn c_min(&self) -> u64 {
        self.c_min
    }

    /// The encoded size in bytes.
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
