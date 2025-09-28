use crate::contract::xl_d_delete::XLDDelete;
use crate::contract::xl_d_insert::XLDInsert;
use crate::contract::xl_d_update::XLDUpdate;
#[cfg(test)]
use arbitrary::Arbitrary;
use mudu::common::bc_dec::{DecErr, Decode, Decoder};
use mudu::common::bc_enc::{EncErr, Encode, Encoder};

#[cfg_attr(any(test, feature = "test"), derive(Arbitrary))]
#[derive(Debug, Eq, PartialEq)]
pub enum XLOp {
    // transaction control op
    CBegin,
    CCommit,
    CAbort,
    // data op
    DInsert(XLDInsert),
    DUpdate(XLDUpdate),
    DDelete(XLDDelete),
}

const INVALID: u8 = 0;
const BEGIN: u8 = 1;
const COMMIT: u8 = 2;
const ABORT: u8 = 3;

const INSERT: u8 = 4;
const UPDATE: u8 = 5;
const DELETE: u8 = 6;

impl Encode for XLOp {
    fn encode<E: Encoder>(&self, encoder: &mut E) -> Result<(), EncErr> {
        match self {
            XLOp::CBegin => {
                encoder.write_u8(BEGIN)?;
            }
            XLOp::CCommit => {
                encoder.write_u8(COMMIT)?;
            }
            XLOp::CAbort => {
                encoder.write_u8(ABORT)?;
            }
            XLOp::DInsert(op) => {
                encoder.write_u8(INSERT)?;
                Encode::encode(op, encoder)?;
            }
            XLOp::DUpdate(op) => {
                encoder.write_u8(UPDATE)?;
                Encode::encode(op, encoder)?;
            }
            XLOp::DDelete(op) => {
                encoder.write_u8(DELETE)?;
                Encode::encode(op, encoder)?;
            }
        }

        Ok(())
    }

    fn size(&self) -> Result<usize, EncErr> {
        let size = size_of::<u8>();
        let n = match self {
            XLOp::CBegin => 0,
            XLOp::CCommit => 0,
            XLOp::CAbort => 0,
            XLOp::DInsert(op) => op.size()?,
            XLOp::DUpdate(op) => op.size()?,
            XLOp::DDelete(op) => op.size()?,
        };
        Ok(size + n)
    }
}

impl Decode for XLOp {
    fn decode<E: Decoder>(decoder: &mut E) -> Result<Self, DecErr> {
        let xl_type: u8 = decoder.read_u8()?;
        let res = match xl_type {
            BEGIN => XLOp::CBegin,
            COMMIT => XLOp::CCommit,
            ABORT => XLOp::CAbort,
            INSERT => {
                let op = XLDInsert::decode(decoder)?;
                XLOp::DInsert(op)
            }
            UPDATE => {
                let op = XLDUpdate::decode(decoder)?;
                XLOp::DUpdate(op)
            }
            DELETE => {
                let op = XLDDelete::decode(decoder)?;
                XLOp::DDelete(op)
            }
            _ => {
                return Err(DecErr::EmptyEnum {
                    type_name: "XLOp".to_string(),
                })
            }
        };
        Ok(res)
    }
}
