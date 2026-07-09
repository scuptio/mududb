#[cfg(test)]
mod tests {
    use crate::data_type_param::DataTypeParamDyn;
    use crate::data_type_param_time::{DataTypeParamTime, TEMPORAL_MAX_PRECISION};
    use mudu::common::cmp_order::Order;

    #[test]
    fn time_new_and_accessors() {
        let t = DataTypeParamTime::new(3);
        assert_eq!(t.precision(), 3);
    }

    #[test]
    fn time_default_precision_is_max() {
        let t = DataTypeParamTime::default();
        assert_eq!(t.precision(), TEMPORAL_MAX_PRECISION);
    }

    #[test]
    fn time_validate_accepts_valid_precision() {
        assert!(DataTypeParamTime::new(0).validate().is_ok());
        assert!(
            DataTypeParamTime::new(TEMPORAL_MAX_PRECISION)
                .validate()
                .is_ok()
        );
    }

    #[test]
    fn time_validate_rejects_excessive_precision() {
        assert!(
            DataTypeParamTime::new(TEMPORAL_MAX_PRECISION + 1)
                .validate()
                .is_err()
        );
    }

    #[test]
    fn time_json_roundtrip() {
        let t = DataTypeParamTime::new(2);
        let json = t.se_to_json().unwrap();
        let mut restored = DataTypeParamTime::default();
        restored.de_from_json(&json).unwrap();
        assert_eq!(restored.precision(), 2);
    }

    #[test]
    fn time_name_contains_type_and_precision() {
        let t = DataTypeParamTime::new(2);
        let name = t.name().to_lowercase();
        assert!(name.contains("time"));
        assert!(name.contains("2"));
    }

    #[test]
    fn time_compare_orders_by_precision() {
        let a = DataTypeParamTime::new(1);
        let b = DataTypeParamTime::new(3);
        assert_eq!(a.cmp_ord(&b).unwrap(), std::cmp::Ordering::Less);
    }
}
