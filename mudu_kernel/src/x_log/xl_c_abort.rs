use arbitrary::Arbitrary;
use mudu::common::bc_dec::{DecErr, Decode, Decoder};
use mudu::common::bc_enc::{EncErr, Encode, Encoder};

#[derive(Arbitrary, Debug, Eq, PartialEq)]
pub struct XLCAbort {}

impl Encode for XLCAbort {
    fn encode<E: Encoder>(&self, _encoder: &mut E) -> Result<(), EncErr> {
        Ok(())
    }

    fn size(&self) -> Result<usize, EncErr> {
        Ok(0)
    }
}

impl Decode for XLCAbort {
    fn decode<E: Decoder>(_decoder: &mut E) -> Result<Self, DecErr> {
        Ok(Self {})
    }
}
