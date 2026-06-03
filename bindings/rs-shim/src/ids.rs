use crate::exports::mududb::component_shim::types;

pub fn to_facade(id: types::Oid) -> mududb::common::id::OID {
    ((id.hi as u128) << 64) | id.lo as u128
}

pub fn from_facade(id: mududb::common::id::OID) -> types::Oid {
    types::Oid {
        hi: (id >> 64) as u64,
        lo: id as u64,
    }
}
