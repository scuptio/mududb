use crate::common::endian;
use crate::common::result::RS;
use crate::error::ErrorCode;
use crate::mudu_error;
pub type OID = u128;

pub const INVALID_OID: OID = 0;
pub fn is_xid_invalid(xid: &OID) -> bool {
    *xid == INVALID_OID
}

pub fn xid_from_binary(binary: &[u8]) -> RS<OID> {
    if binary.len() < size_of::<u128>() {
        return Err(mudu_error!(
            ErrorCode::Internal,
            "cannot decode xid from binary"
        ));
    }
    let xid = endian::read_u128(binary);
    Ok(xid as _)
}

pub fn xid_to_binary(xid: OID) -> Vec<u8> {
    let mut buf = vec![0; size_of::<u128>()];
    endian::write_u128(&mut buf, xid);
    buf
}
