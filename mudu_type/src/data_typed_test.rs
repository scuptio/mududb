#[cfg(test)]
mod tests {
    use crate::data_typed::DataTyped;
    use crate::type_family::TypeFamily;
    use mudu::data_type::date::DateValue;
    use mudu::data_type::numeric::Numeric;
    use mudu::data_type::time::TimeValue;
    use mudu::data_type::timestamp::TimestampValue;
    use mudu::data_type::timestamptz::TimestampTzValue;

    #[test]
    fn from_i32() {
        let typed = DataTyped::from_i32(42);
        assert_eq!(typed.data_type().type_family(), TypeFamily::I32);
        assert_eq!(typed.data_internal().to_i32(), 42);
    }

    #[test]
    fn from_i64() {
        let typed = DataTyped::from_i64(99);
        assert_eq!(typed.data_type().type_family(), TypeFamily::I64);
        assert_eq!(typed.data_internal().to_i64(), 99);
    }

    #[test]
    fn from_i128() {
        let typed = DataTyped::from_i128(170141183460469231731687303715884105727i128);
        assert_eq!(typed.data_type().type_family(), TypeFamily::I128);
        assert_eq!(
            *typed.data_internal().expect_i128(),
            170141183460469231731687303715884105727i128
        );
    }

    #[test]
    fn from_oid() {
        let typed = DataTyped::from_oid(12345u128);
        assert_eq!(typed.data_type().type_family(), TypeFamily::U128);
        assert_eq!(typed.data_internal().to_oid(), 12345u128);
    }

    #[test]
    fn from_f32() {
        let typed = DataTyped::from_f32(1.5);
        assert_eq!(typed.data_type().type_family(), TypeFamily::F32);
        assert_eq!(typed.data_internal().to_f32(), 1.5);
    }

    #[test]
    fn from_f64() {
        let typed = DataTyped::from_f64(2.5);
        assert_eq!(typed.data_type().type_family(), TypeFamily::F64);
        assert_eq!(typed.data_internal().to_f64(), 2.5);
    }

    #[test]
    fn from_string() {
        let typed = DataTyped::from_string("hello".to_string());
        assert_eq!(typed.data_type().type_family(), TypeFamily::String);
        assert_eq!(typed.data_internal().expect_string(), "hello");
    }

    #[test]
    fn from_numeric() {
        let numeric = Numeric::parse("123.456").unwrap();
        let typed = DataTyped::from_numeric(numeric.clone());
        assert_eq!(typed.data_type().type_family(), TypeFamily::Numeric);
        assert_eq!(
            typed.data_internal().expect_numeric().to_plain_string(),
            numeric.to_plain_string()
        );
    }

    #[test]
    fn from_date() {
        let date = DateValue::parse("2026-05-20").unwrap();
        let typed = DataTyped::from_date(date);
        assert_eq!(typed.data_type().type_family(), TypeFamily::Date);
        assert_eq!(
            typed.data_internal().expect_date().days_since_epoch(),
            20_593
        );
    }

    #[test]
    fn from_time() {
        let time = TimeValue::parse("12:34:56.123456").unwrap();
        let typed = DataTyped::from_time(time);
        assert_eq!(typed.data_type().type_family(), TypeFamily::Time);
        assert_eq!(
            typed.data_internal().expect_time().micros_since_midnight(),
            12 * 3600 * 1_000_000 + 34 * 60 * 1_000_000 + 56 * 1_000_000 + 123_456
        );
    }

    #[test]
    fn from_timestamp() {
        let ts = TimestampValue::parse("2026-05-20T14:30:45.123456").unwrap();
        let expected_micros = ts.epoch_micros();
        let typed = DataTyped::from_timestamp(ts);
        assert_eq!(typed.data_type().type_family(), TypeFamily::Timestamp);
        assert_eq!(
            typed.data_internal().expect_timestamp().epoch_micros(),
            expected_micros
        );
    }

    #[test]
    fn from_timestamptz() {
        let tstz = TimestampTzValue::parse("2026-05-20T14:30:45.123456+08:00").unwrap();
        let expected_micros = tstz.epoch_micros_utc();
        let typed = DataTyped::from_timestamptz(tstz);
        assert_eq!(typed.data_type().type_family(), TypeFamily::TimestampTz);
        assert_eq!(
            typed
                .data_internal()
                .expect_timestamptz()
                .epoch_micros_utc(),
            expected_micros
        );
    }

    #[test]
    fn new_and_accessors() {
        let data_type = crate::data_type::DataType::new_no_param(TypeFamily::I64);
        let data_internal = crate::data_value::DataValue::from_i64(7);
        let typed = DataTyped::new(data_type.clone(), data_internal.clone());
        assert_eq!(typed.data_type().type_family(), data_type.type_family());
        assert_eq!(typed.data_internal().to_i64(), data_internal.to_i64());
    }

    #[test]
    fn clone_preserves_contents() {
        let typed = DataTyped::from_string("clone-me".to_string());
        let cloned = typed.clone();
        assert_eq!(
            cloned.data_type().type_family(),
            typed.data_type().type_family()
        );
        assert_eq!(
            cloned.data_internal().expect_string(),
            typed.data_internal().expect_string()
        );
    }
}
