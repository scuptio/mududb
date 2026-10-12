//! Transaction identifier helpers.

use crate::common::endian;
use crate::common::result::RS;
use crate::error::ErrorCode;
use crate::mudu_error;

/// Transaction identifier.
pub type OID = u128;

/// Reserved sentinel for an invalid transaction identifier.
pub const INVALID_OID: OID = 0;

/// Returns `true` if `xid` is the invalid sentinel.
pub fn is_xid_invalid(xid: &OID) -> bool {
    *xid == INVALID_OID
}

/// Decodes a transaction identifier from its 16-byte binary form.
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

/// Encodes a transaction identifier into its 16-byte binary form.
pub fn xid_to_binary(xid: OID) -> Vec<u8> {
    let mut buf = vec![0; size_of::<u128>()];
    endian::write_u128(&mut buf, xid);
    buf
}
