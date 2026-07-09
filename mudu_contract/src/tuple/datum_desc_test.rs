#[cfg(test)]
#[allow(clippy::unwrap_used)]
#[allow(clippy::expect_used)]
#[allow(clippy::panic)]
mod tests {
    use crate::tuple::datum_desc::DatumDesc;
    use mudu_type::data_type::DataType;
    use mudu_type::type_family::TypeFamily;

    fn i32_type() -> DataType {
        DataType::new_no_param(TypeFamily::I32)
    }

    #[test]
    fn datum_desc_new_stores_name_type_and_not_nullable() {
        let desc = DatumDesc::new("id".to_string(), i32_type());
        assert_eq!(desc.name(), "id");
        assert_eq!(desc.type_family(), TypeFamily::I32);
        assert!(!desc.nullable());
    }

    #[test]
    fn datum_desc_new_nullable_preserves_flag() {
        let desc = DatumDesc::new_nullable("name".to_string(), i32_type(), true);
        assert!(desc.nullable());
    }

    #[test]
    fn datum_desc_data_type_returns_reference() {
        let desc = DatumDesc::new("x".to_string(), i32_type());
        assert_eq!(desc.data_type().type_family(), TypeFamily::I32);
    }

    #[test]
    fn datum_desc_into_extracts_name_and_type() {
        let desc = DatumDesc::new("x".to_string(), i32_type());
        let (name, data_type) = desc.into();
        assert_eq!(name, "x");
        assert_eq!(data_type.type_family(), TypeFamily::I32);
    }

    #[test]
    fn datum_desc_serde_roundtrip() {
        let desc = DatumDesc::new_nullable("y".to_string(), i32_type(), true);
        let json = serde_json::to_string(&desc).unwrap();
        let restored: DatumDesc = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.name(), "y");
        assert!(restored.nullable());
        assert_eq!(restored.type_family(), TypeFamily::I32);
    }
}
