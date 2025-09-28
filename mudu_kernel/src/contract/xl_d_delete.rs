#[cfg(test)]
use arbitrary::Arbitrary;
use mudu::common::bc_dec::{DecErr, Decode, Decoder};
use mudu::common::bc_enc::{EncErr, Encode, Encoder};
use mudu::common::buf::Buf;
use mudu::common::id::OID;

// delete key value
#[cfg_attr(any(test, feature = "test"), derive(Arbitrary))]
#[derive(Debug, Eq, PartialEq)]
pub struct XLDDelete {
    table_id: OID,
    tuple_id: OID,
    key: Buf,
}

impl XLDDelete {
    pub fn new(table_id: OID, tuple_id: OID, key: Buf) -> Self {
        Self {
            table_id,
            tuple_id,
            key,
        }
    }
}
impl Encode for XLDDelete {
    fn encode<E: Encoder>(&self, encoder: &mut E) -> Result<(), EncErr> {
        encoder.write_u128(self.table_id)?;
        encoder.write_u128(self.tuple_id)?;
        encoder.write_u32(self.key.len() as u32)?;
        encoder.write_bytes(self.key.as_slice())?;
        Ok(())
    }

    fn size(&self) -> Result<usize, EncErr> {
        let mut size = 0usize;
        size += size_of_val(&self.table_id);
        size += size_of_val(&self.tuple_id);
        size += size_of::<u32>();
        size += self.key.len();
        Ok(size)
    }
}

impl Decode for XLDDelete {
    fn decode<E: Decoder>(decoder: &mut E) -> Result<Self, DecErr> {
        let table_id = decoder.read_u128()?;
        let tuple_id = decoder.read_u128()?;
        let mut key = Buf::new();
        let len: u32 = decoder.read_u32()?;
        key.resize(len as usize, 0);
        decoder.read_bytes(key.as_mut_slice())?;

        Ok(Self {
            table_id,
            tuple_id,
            key,
        })
    }
}
