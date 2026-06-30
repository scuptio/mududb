#[cfg(test)]
#[allow(clippy::unwrap_used)]
#[allow(clippy::expect_used)]
mod tests {
    use crate::dat_type::DatType;
    use crate::dat_type_id::DatTypeID;
    use crate::scalar_type::ScalarType;

    #[test]
    fn test_new_without_param_for_scalar_types() {
        for id in [
            DatTypeID::I32,
            DatTypeID::I64,
            DatTypeID::F32,
            DatTypeID::F64,
            DatTypeID::String,
            DatTypeID::Date,
            DatTypeID::Time,
            DatTypeID::Timestamp,
            DatTypeID::TimestampTz,
        ] {
            let scalar = ScalarType::new_without_param(id);
            assert_eq!(scalar.id(), id);
            assert_eq!(scalar.type_obj().dat_type_id(), id);
            assert!(!scalar.has_param());
        }
    }

    #[test]
    fn test_new_default_for_scalar_types() {
        for id in [DatTypeID::I32, DatTypeID::String, DatTypeID::Numeric] {
            let scalar = ScalarType::new_default(id);
            assert_eq!(scalar.id(), id);
        }
    }

    #[test]
    fn test_new_from_dat_type() {
        let dat_type = DatType::new_no_param(DatTypeID::I64);
        let scalar = ScalarType::new(dat_type.clone());
        assert_eq!(scalar.id(), DatTypeID::I64);
        assert_eq!(scalar.type_obj().dat_type_id(), DatTypeID::I64);
        assert!(!scalar.has_param());
    }

    #[test]
    fn test_param_info_for_no_param() {
        let scalar = ScalarType::new_without_param(DatTypeID::I32);
        let info = scalar.param_info();
        assert_eq!(info.id, DatTypeID::I32);
    }

    #[test]
    #[should_panic(expected = "ScalarType id must be scalar type")]
    fn test_new_without_param_panics_for_array() {
        ScalarType::new_without_param(DatTypeID::Array);
    }

    #[test]
    #[should_panic(expected = "ScalarType id must be scalar type")]
    fn test_new_default_panics_for_record() {
        ScalarType::new_default(DatTypeID::Record);
    }

    #[test]
    #[should_panic(expected = "ScalarType id must be scalar type")]
    fn test_new_panics_for_binary() {
        ScalarType::new(DatType::new_no_param(DatTypeID::Binary));
    }

    #[test]
    fn test_serde_round_trip() {
        let original = ScalarType::new_without_param(DatTypeID::String);
        let serialized = serde_json::to_string(&original).unwrap();
        let deserialized: ScalarType = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized.id(), DatTypeID::String);
    }
}
