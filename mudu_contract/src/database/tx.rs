use mudu::common::xid::XID;

pub trait Tx {
    fn xid(&self) -> XID;
}
