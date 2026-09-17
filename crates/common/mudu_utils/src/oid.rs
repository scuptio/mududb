use mudu::common::id::OID;

pub fn gen_oid() -> OID {
    mudu_sys::random::uuid_v4().as_u128()
}

pub fn new_xid() -> OID {
    mudu_sys::random::uuid_v4().as_u128()
}

#[cfg(test)]
mod tests {
    use super::{gen_oid, new_xid};

    #[test]
    fn gen_oid_is_non_zero_u128() {
        let oid = gen_oid();
        assert_ne!(oid, 0);
    }

    #[test]
    fn new_xid_is_non_zero_u128() {
        let xid = new_xid();
        assert_ne!(xid, 0);
    }

    #[test]
    fn gen_oid_produces_distinct_values() {
        let a = gen_oid();
        let b = gen_oid();
        assert_ne!(a, b);
    }

    #[test]
    fn new_xid_produces_distinct_values() {
        let a = new_xid();
        let b = new_xid();
        assert_ne!(a, b);
    }

    #[test]
    fn oid_and_xid_share_u128_format() {
        let oid = gen_oid();
        let xid = new_xid();
        assert_eq!(std::mem::size_of_val(&oid), 16);
        assert_eq!(std::mem::size_of_val(&xid), 16);
        assert_ne!(oid, xid);
    }
}
