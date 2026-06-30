#[cfg(test)]
mod tests {
    use crate::dt_param::DTPDyn;
    use crate::dtp_time::TEMPORAL_MAX_PRECISION;
    use crate::dtp_timestamp::DTPTimestamp;
    use mudu::common::cmp_order::Order;

    #[test]
    fn timestamp_new_and_accessors() {
        let t = DTPTimestamp::new(3);
        assert_eq!(t.precision(), 3);
    }

    #[test]
    fn timestamp_default_precision_is_max() {
        let t = DTPTimestamp::default();
        assert_eq!(t.precision(), TEMPORAL_MAX_PRECISION);
    }

    #[test]
    fn timestamp_validate_accepts_valid_precision() {
        assert!(DTPTimestamp::new(0).validate().is_ok());
        assert!(DTPTimestamp::new(TEMPORAL_MAX_PRECISION).validate().is_ok());
    }

    #[test]
    fn timestamp_validate_rejects_excessive_precision() {
        assert!(
            DTPTimestamp::new(TEMPORAL_MAX_PRECISION + 1)
                .validate()
                .is_err()
        );
    }

    #[test]
    fn timestamp_json_roundtrip() {
        let t = DTPTimestamp::new(2);
        let json = t.se_to_json().unwrap();
        let mut restored = DTPTimestamp::default();
        restored.de_from_json(&json).unwrap();
        assert_eq!(restored.precision(), 2);
    }

    #[test]
    fn timestamp_name_contains_type_and_precision() {
        let t = DTPTimestamp::new(2);
        let name = t.name().to_lowercase();
        assert!(name.contains("timestamp"));
        assert!(name.contains("2"));
    }

    #[test]
    fn timestamp_compare_orders_by_precision() {
        let a = DTPTimestamp::new(1);
        let b = DTPTimestamp::new(3);
        assert_eq!(a.cmp_ord(&b).unwrap(), std::cmp::Ordering::Less);
    }
}
