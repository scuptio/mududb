use crate::common::xid::XID;

pub trait Tx {
    fn xid(&self) -> XID;
}

