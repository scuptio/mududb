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

impl From<UniOid> for OID {
    fn from(val: UniOid) -> Self {
        val.to_oid()
    }
}

impl From<OID> for UniOid {
    fn from(oid: OID) -> Self {
        Self::from_oid(oid)
    }
}

#[cfg(test)]
mod tests {
    use super::UniOid;
    use mudu::common::id::OID;

    #[test]
    fn from_oid_splits_128_bit_oid() {
        let oid: OID = 0x0001_0002_0003_0004_0005_0006_0007_0008u128;
        let uni = UniOid::from_oid(oid);
        assert_eq!(uni.h, 0x0001_0002_0003_0004u64);
        assert_eq!(uni.l, 0x0005_0006_0007_0008u64);
    }

    #[test]
    fn to_oid_recombines_128_bit_oid() {
        let uni = UniOid {
            h: 0x0102_0304_0506_0708u64,
            l: 0x090a_0b0c_0d0e_0f00u64,
        };
        assert_eq!(uni.to_oid(), 0x0102_0304_0506_0708_090a_0b0c_0d0e_0f00u128);
    }

    #[test]
    fn from_oid_to_oid_roundtrip() {
        let oid: OID = 0xdead_beef_cafe_babe_1234_5678_9abc_def0u128;
        let round = UniOid::from_oid(oid).to_oid();
        assert_eq!(round, oid);
    }

    #[test]
    fn from_impls_roundtrip() {
        let oid: OID = 42u128;
        let uni: UniOid = oid.into();
        let back: OID = uni.into();
        assert_eq!(back, oid);
    }
}
