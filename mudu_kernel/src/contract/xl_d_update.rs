#[cfg(test)]
use arbitrary::Arbitrary;
use mudu::common::bc_dec::{DecErr, Decode, Decoder};
use mudu::common::bc_enc::{EncErr, Encode, Encoder};
use mudu::common::buf::Buf;
use mudu::common::id::OID;
use mudu::common::update_delta::UpdateDelta;

#[cfg_attr(any(test, feature = "test"), derive(Arbitrary))]
#[derive(Debug, Eq, PartialEq)]
pub struct XLDUpdate {
    table_id: OID,
    tuple_id: OID,
    key: Buf,
    delta: Vec<UpdateDelta>,
}

impl XLDUpdate {
    pub fn new(table_id: OID, tuple_id: OID, key: Buf, delta: Vec<UpdateDelta>) -> XLDUpdate {
        Self {
            table_id,
            tuple_id,
            key,
            delta,
        }
    }

    pub fn table_id(&self) -> OID {
        self.table_id
    }

    pub fn tuple_id(&self) -> OID {
        self.tuple_id
    }

    pub fn key(&self) -> &Buf {
        &self.key
    }

    pub fn delta(&self) -> &Vec<UpdateDelta> {
        &self.delta
    }
}

impl Encode for XLDUpdate {
    fn encode<E: Encoder>(&self, encoder: &mut E) -> Result<(), EncErr> {
        encoder.write_u128(self.table_id)?;
        encoder.write_u128(self.tuple_id)?;
        encoder.write_u32(self.key.len() as u32)?;
        encoder.write_bytes(self.key.as_slice())?;
        encoder.write_u32(self.delta.len() as u32)?;
        for d in self.delta.iter() {
            Encode::encode(d, encoder)?;
        }
        Ok(())
    }

    fn size(&self) -> Result<usize, EncErr> {
        let mut size = 0;
        size += size_of_val(&self.table_id);
        size += size_of_val(&self.tuple_id);
        size += size_of::<u32>();
        size += self.key.len();
        size += size_of::<u32>(); // delta len
        for d in self.delta.iter() {
            size += d.size()?;
        }
        Ok(size)
    }
}

impl Decode for XLDUpdate {
    fn decode<E: Decoder>(decoder: &mut E) -> Result<Self, DecErr> {
        let table_id = decoder.read_u128()?;
        let tuple_id = decoder.read_u128()?;
        let mut key = Buf::new();
        let len: u32 = decoder.read_u32()?;
        key.resize(len as usize, 0);
        decoder.read_bytes(key.as_mut_slice())?;
        let num_delta = decoder.read_u32()?;
        let mut delta = vec![];
        for _i in 0..num_delta {
            let data = UpdateDelta::decode(decoder)?;
            delta.push(data);
        }

        Ok(Self {
            table_id,
            tuple_id,
            key,
            delta,
        })
    }
}
