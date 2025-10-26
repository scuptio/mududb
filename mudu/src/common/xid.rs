use uuid::Uuid;

pub type XID = u128;

pub const INVALID_XID: XID = 0;
pub fn new_xid() -> XID {
    let id = Uuid::new_v4();
    id.as_u128()
}

pub fn is_xid_invalid(xid: &XID) -> bool {
    *xid == INVALID_XID
}
