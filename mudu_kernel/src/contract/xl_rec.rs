use crate::contract::xl_d_delete::XLDDelete;
use crate::contract::xl_d_insert::XLDInsert;
use crate::contract::xl_d_update::XLDUpdate;
use crate::contract::xl_op::XLOp;
#[cfg(any(test, feature = "test"))]
use arbitrary::Arbitrary;
use mudu::common::bc_dec::{DecErr, Decode, Decoder};
use mudu::common::bc_enc::{EncErr, Encode, Encoder};
use mudu::common::buf::Buf;
use mudu::common::id::OID;
use mudu::common::update_delta::UpdateDelta;
use mudu::common::xid::XID;
use std::mem::size_of;

#[cfg_attr(any(test, feature = "test"), derive(Arbitrary))]
#[derive(Debug, Eq, PartialEq)]
pub struct XLRec {
    xid: XID,
    ops: Vec<XLOp>,
}

impl XLRec {
    pub fn new(xid: XID) -> XLRec {
        let ops = vec![];
        Self { xid, ops }
    }

    pub fn add_insert(&mut self, table_id: OID, tuple_id: OID, key: Buf, value: Buf) {
        let op = XLOp::DInsert(XLDInsert::new(table_id, tuple_id, key, value));
        self.ops.push(op);
    }

    pub fn add_update(&mut self, table_id: OID, tuple_id: OID, key: Buf, value: Vec<UpdateDelta>) {
        let op = XLOp::DUpdate(XLDUpdate::new(table_id, tuple_id, key, value));
        self.ops.push(op);
    }

    pub fn add_delete(&mut self, table_id: OID, tuple_id: OID, key: Buf) {
        let op = XLOp::DDelete(XLDDelete::new(table_id, tuple_id, key));
        self.ops.push(op);
    }

    pub fn commit(&mut self) {
        let op = XLOp::CCommit;
        self.ops.push(op)
    }
}

impl Encode for XLRec {
    fn encode<E: Encoder>(&self, encoder: &mut E) -> Result<(), EncErr> {
        encoder.write_u128(self.xid)?;
        let len = self.ops.len() as u32;
        encoder.write_u32(len)?;
        for x in self.ops.iter() {
            Encode::encode(x, encoder)?
        }
        Ok(())
    }

    fn size(&self) -> Result<usize, EncErr> {
        let mut len = 0;
        len += size_of::<u64>();
        len += size_of::<u32>();
        for x in self.ops.iter() {
            let n = Encode::size(x)?;
            len += n;
        }
        Ok(len)
    }
}

impl Decode for XLRec {
    fn decode<E: Decoder>(decoder: &mut E) -> Result<Self, DecErr> {
        let xid = decoder.read_u128()?;
        let mut ops = vec![];
        let len = decoder.read_u32()? as usize;
        for _i in 0..len {
            let op = Decode::decode(decoder)?;
            ops.push(op);
        }
        let res = Self { xid, ops };
        Ok(res)
    }
}
