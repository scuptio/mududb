use mudu::common::id::OID;

pub trait Tx {
    fn xid(&self) -> OID;
}
