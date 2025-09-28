use uuid::Uuid;

pub type XID = u128;

pub fn new_xid() -> XID {
    let id = Uuid::new_v4();
    id.as_u128()
}