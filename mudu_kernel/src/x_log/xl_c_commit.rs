use arbitrary::Arbitrary;
use mudu::common::bc_dec::{DecErr, Decode, Decoder};
use mudu::common::bc_enc::{EncErr, Encode, Encoder};

#[derive(Arbitrary, Debug, Eq, PartialEq)]
pub struct XLCCommit {}

impl Encode for XLCCommit {
    fn encode<E: Encoder>(&self, _encoder: &mut E) -> Result<(), EncErr> {
        Ok(())
    }

    fn size(&self) -> Result<usize, EncErr> {
        let len = 0;
        Ok(len)
    }
}

impl Decode for XLCCommit {
    fn decode<E: Decoder>(_decoder: &mut E) -> Result<Self, DecErr> {
        Ok(Self {})
    }
}

impl XLCCommit {
    fn new() -> Self {
        Self {}
    }
}
