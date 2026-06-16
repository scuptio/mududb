use mudu::common::id::OID;

pub fn gen_oid() -> OID {
    mudu_sys::random::uuid_v4().as_u128()
}

pub fn new_xid() -> OID {
    mudu_sys::random::uuid_v4().as_u128()
}
