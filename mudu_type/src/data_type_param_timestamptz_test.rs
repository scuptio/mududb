#[cfg(test)]
mod tests {
    use crate::data_type_param::DataTypeParamDyn;
    use crate::data_type_param_time::TEMPORAL_MAX_PRECISION;
    use crate::data_type_param_timestamptz::DataTypeParamTimestampTz;
    use mudu::common::cmp_order::Order;

    #[test]
    fn timestamptz_new_and_accessors() {
        let t = DataTypeParamTimestampTz::new(3);
        assert_eq!(t.precision(), 3);
    }

    #[test]
    fn timestamptz_default_precision_is_max() {
        let t = DataTypeParamTimestampTz::default();
        assert_eq!(t.precision(), TEMPORAL_MAX_PRECISION);
    }

    #[test]
    fn timestamptz_validate_accepts_valid_precision() {
        assert!(DataTypeParamTimestampTz::new(0).validate().is_ok());
        assert!(
            DataTypeParamTimestampTz::new(TEMPORAL_MAX_PRECISION)
                .validate()
                .is_ok()
        );
    }

    #[test]
    fn timestamptz_validate_rejects_excessive_precision() {
        assert!(
            DataTypeParamTimestampTz::new(TEMPORAL_MAX_PRECISION + 1)
                .validate()
                .is_err()
        );
    }

    #[test]
    fn timestamptz_json_roundtrip() {
        let t = DataTypeParamTimestampTz::new(2);
        let json = t.se_to_json().unwrap();
        let mut restored = DataTypeParamTimestampTz::default();
        restored.de_from_json(&json).unwrap();
        assert_eq!(restored.precision(), 2);
    }

    #[test]
    fn timestamptz_name_contains_type_and_precision() {
        let t = DataTypeParamTimestampTz::new(2);
        let name = t.name().to_lowercase();
        assert!(name.contains("timestamp"));
        assert!(name.contains("2"));
    }

    #[test]
    fn timestamptz_compare_orders_by_precision() {
        let a = DataTypeParamTimestampTz::new(1);
        let b = DataTypeParamTimestampTz::new(3);
        assert_eq!(a.cmp_ord(&b).unwrap(), std::cmp::Ordering::Less);
    }
}
