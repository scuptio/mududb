#[cfg(test)]
#[allow(clippy::unwrap_used)]
#[allow(clippy::expect_used)]
#[allow(clippy::panic)]
mod tests {
    use crate::tuple::datum_desc::DatumDesc;
    use mudu_type::dat_type::DatType;
    use mudu_type::dat_type_id::DatTypeID;

    fn i32_type() -> DatType {
        DatType::new_no_param(DatTypeID::I32)
    }

    #[test]
    fn datum_desc_new_stores_name_type_and_not_nullable() {
        let desc = DatumDesc::new("id".to_string(), i32_type());
        assert_eq!(desc.name(), "id");
        assert_eq!(desc.dat_type_id(), DatTypeID::I32);
        assert!(!desc.nullable());
    }

    #[test]
    fn datum_desc_new_nullable_preserves_flag() {
        let desc = DatumDesc::new_nullable("name".to_string(), i32_type(), true);
        assert!(desc.nullable());
    }

    #[test]
    fn datum_desc_dat_type_returns_reference() {
        let desc = DatumDesc::new("x".to_string(), i32_type());
        assert_eq!(desc.dat_type().dat_type_id(), DatTypeID::I32);
    }

    #[test]
    fn datum_desc_into_extracts_name_and_type() {
        let desc = DatumDesc::new("x".to_string(), i32_type());
        let (name, dat_type) = desc.into();
        assert_eq!(name, "x");
        assert_eq!(dat_type.dat_type_id(), DatTypeID::I32);
    }

    #[test]
    fn datum_desc_serde_roundtrip() {
        let desc = DatumDesc::new_nullable("y".to_string(), i32_type(), true);
        let json = serde_json::to_string(&desc).unwrap();
        let restored: DatumDesc = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.name(), "y");
        assert!(restored.nullable());
        assert_eq!(restored.dat_type_id(), DatTypeID::I32);
    }
}
