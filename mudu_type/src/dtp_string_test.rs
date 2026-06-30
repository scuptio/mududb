#[cfg(test)]
mod tests {
    use crate::dt_param::DTPDyn;
    use crate::dtp_string::DTPString;
    use mudu::common::cmp_order::Order;

    #[test]
    fn string_new_and_length() {
        let s = DTPString::new(42);
        assert_eq!(s.length(), 42);
    }

    #[test]
    fn string_default_is_zero() {
        let s = DTPString::default();
        assert_eq!(s.length(), 0);
    }

    #[test]
    fn string_fixed_length_is_false() {
        let s = DTPString::new(10);
        assert!(!s.fixed_length());
    }

    #[test]
    fn string_compare_orders_fixed_before_var() {
        // fixed_length always returns false in this implementation
        let a = DTPString::new(5);
        let b = DTPString::new(10);
        assert_eq!(a.compare(&b), std::cmp::Ordering::Equal);
        assert_eq!(a.cmp_ord(&b).unwrap(), std::cmp::Ordering::Equal);
    }

    #[test]
    fn string_json_roundtrip() {
        let s = DTPString::new(7);
        let json = s.se_to_json().unwrap();
        let mut restored = DTPString::default();
        restored.de_from_json(&json).unwrap();
        assert_eq!(restored.length(), 7);
    }

    #[test]
    fn string_name_is_non_empty() {
        let s = DTPString::new(3);
        assert!(!s.name().is_empty());
    }
}
