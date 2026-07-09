#[cfg(test)]
#[allow(clippy::unwrap_used)]
#[allow(clippy::expect_used)]
mod tests {
    use crate::data_type::DataType;
    use crate::scalar_type::ScalarType;
    use crate::type_family::TypeFamily;

    #[test]
    fn test_new_without_param_for_scalar_types() {
        for id in [
            TypeFamily::I32,
            TypeFamily::I64,
            TypeFamily::F32,
            TypeFamily::F64,
            TypeFamily::String,
            TypeFamily::Date,
            TypeFamily::Time,
            TypeFamily::Timestamp,
            TypeFamily::TimestampTz,
        ] {
            let scalar = ScalarType::new_without_param(id);
            assert_eq!(scalar.id(), id);
            assert_eq!(scalar.type_obj().type_family(), id);
            assert!(!scalar.has_param());
        }
    }

    #[test]
    fn test_new_default_for_scalar_types() {
        for id in [TypeFamily::I32, TypeFamily::String, TypeFamily::Numeric] {
            let scalar = ScalarType::new_default(id);
            assert_eq!(scalar.id(), id);
        }
    }

    #[test]
    fn test_new_from_data_type() {
        let data_type = DataType::new_no_param(TypeFamily::I64);
        let scalar = ScalarType::new(data_type.clone());
        assert_eq!(scalar.id(), TypeFamily::I64);
        assert_eq!(scalar.type_obj().type_family(), TypeFamily::I64);
        assert!(!scalar.has_param());
    }

    #[test]
    fn test_param_info_for_no_param() {
        let scalar = ScalarType::new_without_param(TypeFamily::I32);
        let info = scalar.param_info();
        assert_eq!(info.id, TypeFamily::I32);
    }

    #[test]
    #[should_panic(expected = "ScalarType id must be scalar type")]
    fn test_new_without_param_panics_for_array() {
        ScalarType::new_without_param(TypeFamily::Array);
    }

    #[test]
    #[should_panic(expected = "ScalarType id must be scalar type")]
    fn test_new_default_panics_for_record() {
        ScalarType::new_default(TypeFamily::Record);
    }

    #[test]
    #[should_panic(expected = "ScalarType id must be scalar type")]
    fn test_new_panics_for_binary() {
        ScalarType::new(DataType::new_no_param(TypeFamily::Binary));
    }

    #[test]
    fn test_serde_round_trip() {
        let original = ScalarType::new_without_param(TypeFamily::String);
        let serialized = serde_json::to_string(&original).unwrap();
        let deserialized: ScalarType = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized.id(), TypeFamily::String);
    }
}
