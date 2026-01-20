use crate::universal::uni_oid::UniOid;
use mudu::common::id::OID;

impl UniOid {
    pub fn from_oid(oid: OID) -> Self {
        let h = (oid >> 64) as u64;
        let l = (oid & ((1 << 64) - 1)) as u64;
        Self { h, l }
    }

    pub fn to_oid(&self) -> OID {
        ((self.h as u128) << 64) | (self.l as u128)
    }
}

impl Into<OID> for UniOid {
    fn into(self) -> OID {
        self.to_oid()
    }
}

impl From<OID> for UniOid {
    fn from(oid: OID) -> Self {
        Self::from_oid(oid)
    }
}
