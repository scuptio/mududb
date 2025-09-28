#[cfg(test)]
use arbitrary::Arbitrary;
use mudu::common::bc_dec::{DecErr, Decode, Decoder};
use mudu::common::bc_enc::{EncErr, Encode, Encoder};
use mudu::common::buf::Buf;
use mudu::common::id::OID;

// insert or replace a key value pair
#[cfg_attr(any(test, feature = "test"), derive(Arbitrary))]
#[derive(Debug, Eq, PartialEq)]
pub struct XLDInsert {
    table_id: OID,
    tuple_id: OID,
    key: Buf,
    value: Buf,
}

impl XLDInsert {
    pub fn new(table_id: OID, tuple_id: OID, key: Buf, value: Buf) -> XLDInsert {
        Self {
            table_id,
            tuple_id,
            key,
            value,
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

    pub fn value(&self) -> &Buf {
        &self.value
    }
}

impl Encode for XLDInsert {
    fn encode<E: Encoder>(&self, encoder: &mut E) -> Result<(), EncErr> {
        encoder.write_u128(self.table_id)?;
        encoder.write_u128(self.tuple_id)?;
        encoder.write_u32(self.key.len() as u32)?;
        encoder.write_bytes(self.key.as_slice())?;
        encoder.write_u32(self.value.len() as u32)?;
        encoder.write_bytes(self.value.as_slice())?;
        Ok(())
    }

    fn size(&self) -> Result<usize, EncErr> {
        let mut size = 0;
        size += size_of_val(&self.table_id);
        size += size_of_val(&self.tuple_id);
        size += size_of::<u32>(); // key length
        size += self.key.len(); // key
        size += size_of::<u32>(); // value length
        size += self.value.len(); // value
        Ok(size)
    }
}

impl Decode for XLDInsert {
    fn decode<E: Decoder>(decoder: &mut E) -> Result<Self, DecErr> {
        let table_id = decoder.read_u128()?;
        let tuple_id = decoder.read_u128()?;
        let mut key = Buf::new();
        let mut value = Buf::new();
        let len: u32 = decoder.read_u32()?;

        key.resize(len as usize, 0);
        decoder.read_bytes(key.as_mut_slice())?;

        let len: u32 = decoder.read_u32()?;
        value.resize(len as usize, 0);
        decoder.read_bytes(value.as_mut_slice())?;
        Ok(Self {
            table_id,
            tuple_id,
            key,
            value,
        })
    }
}
