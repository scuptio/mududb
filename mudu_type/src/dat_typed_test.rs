#[cfg(test)]
mod tests {
    use crate::dat_type_id::DatTypeID;
    use crate::dat_typed::DatTyped;
    use mudu::data_type::date::DateValue;
    use mudu::data_type::numeric::Numeric;
    use mudu::data_type::time::TimeValue;
    use mudu::data_type::timestamp::TimestampValue;
    use mudu::data_type::timestamptz::TimestampTzValue;

    #[test]
    fn from_i32() {
        let typed = DatTyped::from_i32(42);
        assert_eq!(typed.dat_type().dat_type_id(), DatTypeID::I32);
        assert_eq!(typed.dat_internal().to_i32(), 42);
    }

    #[test]
    fn from_i64() {
        let typed = DatTyped::from_i64(99);
        assert_eq!(typed.dat_type().dat_type_id(), DatTypeID::I64);
        assert_eq!(typed.dat_internal().to_i64(), 99);
    }

    #[test]
    fn from_i128() {
        let typed = DatTyped::from_i128(170141183460469231731687303715884105727i128);
        assert_eq!(typed.dat_type().dat_type_id(), DatTypeID::I128);
        assert_eq!(
            *typed.dat_internal().expect_i128(),
            170141183460469231731687303715884105727i128
        );
    }

    #[test]
    fn from_oid() {
        let typed = DatTyped::from_oid(12345u128);
        assert_eq!(typed.dat_type().dat_type_id(), DatTypeID::U128);
        assert_eq!(typed.dat_internal().to_oid(), 12345u128);
    }

    #[test]
    fn from_f32() {
        let typed = DatTyped::from_f32(1.5);
        assert_eq!(typed.dat_type().dat_type_id(), DatTypeID::F32);
        assert_eq!(typed.dat_internal().to_f32(), 1.5);
    }

    #[test]
    fn from_f64() {
        let typed = DatTyped::from_f64(2.5);
        assert_eq!(typed.dat_type().dat_type_id(), DatTypeID::F64);
        assert_eq!(typed.dat_internal().to_f64(), 2.5);
    }

    #[test]
    fn from_string() {
        let typed = DatTyped::from_string("hello".to_string());
        assert_eq!(typed.dat_type().dat_type_id(), DatTypeID::String);
        assert_eq!(typed.dat_internal().expect_string(), "hello");
    }

    #[test]
    fn from_numeric() {
        let numeric = Numeric::parse("123.456").unwrap();
        let typed = DatTyped::from_numeric(numeric.clone());
        assert_eq!(typed.dat_type().dat_type_id(), DatTypeID::Numeric);
        assert_eq!(
            typed.dat_internal().expect_numeric().to_plain_string(),
            numeric.to_plain_string()
        );
    }

    #[test]
    fn from_date() {
        let date = DateValue::parse("2026-05-20").unwrap();
        let typed = DatTyped::from_date(date);
        assert_eq!(typed.dat_type().dat_type_id(), DatTypeID::Date);
        assert_eq!(
            typed.dat_internal().expect_date().days_since_epoch(),
            20_593
        );
    }

    #[test]
    fn from_time() {
        let time = TimeValue::parse("12:34:56.123456").unwrap();
        let typed = DatTyped::from_time(time);
        assert_eq!(typed.dat_type().dat_type_id(), DatTypeID::Time);
        assert_eq!(
            typed.dat_internal().expect_time().micros_since_midnight(),
            12 * 3600 * 1_000_000 + 34 * 60 * 1_000_000 + 56 * 1_000_000 + 123_456
        );
    }

    #[test]
    fn from_timestamp() {
        let ts = TimestampValue::parse("2026-05-20T14:30:45.123456").unwrap();
        let expected_micros = ts.epoch_micros();
        let typed = DatTyped::from_timestamp(ts);
        assert_eq!(typed.dat_type().dat_type_id(), DatTypeID::Timestamp);
        assert_eq!(
            typed.dat_internal().expect_timestamp().epoch_micros(),
            expected_micros
        );
    }

    #[test]
    fn from_timestamptz() {
        let tstz = TimestampTzValue::parse("2026-05-20T14:30:45.123456+08:00").unwrap();
        let expected_micros = tstz.epoch_micros_utc();
        let typed = DatTyped::from_timestamptz(tstz);
        assert_eq!(typed.dat_type().dat_type_id(), DatTypeID::TimestampTz);
        assert_eq!(
            typed.dat_internal().expect_timestamptz().epoch_micros_utc(),
            expected_micros
        );
    }

    #[test]
    fn new_and_accessors() {
        let dat_type = crate::dat_type::DatType::new_no_param(DatTypeID::I64);
        let dat_internal = crate::dat_value::DatValue::from_i64(7);
        let typed = DatTyped::new(dat_type.clone(), dat_internal.clone());
        assert_eq!(typed.dat_type().dat_type_id(), dat_type.dat_type_id());
        assert_eq!(typed.dat_internal().to_i64(), dat_internal.to_i64());
    }

    #[test]
    fn clone_preserves_contents() {
        let typed = DatTyped::from_string("clone-me".to_string());
        let cloned = typed.clone();
        assert_eq!(
            cloned.dat_type().dat_type_id(),
            typed.dat_type().dat_type_id()
        );
        assert_eq!(
            cloned.dat_internal().expect_string(),
            typed.dat_internal().expect_string()
        );
    }
}
