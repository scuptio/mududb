use crate::common::endian;
use crate::common::result::RS;
use crate::error::ec::EC;
use crate::m_error;
use uuid::Uuid;

pub type OID = u128;

pub const INVALID_OID: OID = 0;
pub fn new_xid() -> OID {
    let id = Uuid::new_v4();
    id.as_u128()
}

pub fn is_xid_invalid(xid: &OID) -> bool {
    *xid == INVALID_OID
}

pub fn xid_from_binary(binary: &[u8]) -> RS<OID> {
    if binary.len() < size_of::<u128>() {
        return Err(m_error!(EC::InternalErr, "cannot decode xid from binary"));
    }
    let xid = endian::read_u128(binary);
    Ok(xid as _)
}

pub fn xid_to_binary(xid: OID) -> Vec<u8> {
    let mut buf = Vec::with_capacity(size_of::<u128>());
    unsafe { buf.set_len(size_of::<u128>()) };
    endian::write_u128(&mut buf, xid);
    buf
}
