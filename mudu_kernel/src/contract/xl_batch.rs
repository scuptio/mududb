use crate::contract::xl_rec::XLRec;
#[cfg(any(test, feature = "test"))]
use arbitrary::Arbitrary;
use mudu::common::bc_dec::{DecErr, Decode, Decoder};
use mudu::common::bc_enc::{EncErr, Encode, Encoder};

#[cfg_attr(any(test, feature = "test"), derive(Arbitrary))]
#[derive(Debug, Eq, PartialEq)]
pub struct XLBatch {
    lsn: u64,
    records: Vec<XLRec>,
}

impl XLBatch {
    pub fn new(lsn: u64, records: Vec<XLRec>) -> XLBatch {
        XLBatch { lsn, records }
    }

    pub fn lsn(&self) -> u64 {
        self.lsn
    }

    pub fn records(&self) -> &Vec<XLRec> {
        &self.records
    }

    pub fn into_records(self) -> Vec<XLRec> {
        self.records
    }
}

impl Encode for XLBatch {
    fn encode<E: Encoder>(&self, encoder: &mut E) -> Result<(), EncErr> {
        encoder.write_u64(self.lsn)?;
        let len = self.records.len() as u32;
        encoder.write_u32(len)?;
        for r in self.records.iter() {
            Encode::encode(r, encoder)?
        }
        Ok(())
    }

    fn size(&self) -> Result<usize, EncErr> {
        let mut len = size_of::<u64>() + size_of::<u32>();
        for r in self.records.iter() {
            len += r.size()?
        }
        Ok(len)
    }
}

impl Decode for XLBatch {
    fn decode<E: Decoder>(decoder: &mut E) -> Result<Self, DecErr> {
        let lsn = decoder.read_u64()?;
        let len = decoder.read_u32()? as usize;
        let mut records = vec![];
        for _i in 0..len {
            let rec = XLRec::decode(decoder)?;
            records.push(rec);
        }
        Ok(Self { lsn, records })
    }
}


#[cfg(test)]
pub mod _fuzz {
    use crate::contract::xl_batch::XLBatch;
    use mudu::common::_arb_de_en::_fuzz_decode_and_encode;

    pub fn _dc_en_x_l_batch(data: &[u8]) {
        _fuzz_decode_and_encode::<XLBatch>(data);
    }
}
