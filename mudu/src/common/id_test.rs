#[cfg(test)]
mod tests {
    use crate::common::id::{AttrIndex, DatumIndex, INVALID_OID, OID, TupleID, oid_cast_to_u32};

    #[test]
    fn invalid_oid_is_zero() {
        assert_eq!(INVALID_OID, 0);
    }

    #[test]
    fn oid_cast_to_u32_truncates() {
        assert_eq!(oid_cast_to_u32(0), 0);
        assert_eq!(oid_cast_to_u32(0x1234_5678_9ABC_DEF0), 0x9ABC_DEF0);
        assert_eq!(oid_cast_to_u32(u128::MAX), 0xFFFF_FFFF);
    }

    #[test]
    fn type_aliases_compile() {
        let _idx: AttrIndex = 0;
        let _didx: DatumIndex = 1;
        let _tid: TupleID = 2;
        let _oid: OID = 3;
    }
}
