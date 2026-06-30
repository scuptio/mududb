#[cfg(test)]
#[allow(clippy::unwrap_used)]
#[allow(clippy::expect_used)]
#[allow(clippy::panic)]
mod tests {
    use crate::tuple::datum_convert::{
        datum_from_binary, datum_from_value, datum_to_binary, datum_to_value,
    };
    use crate::tuple::datum_desc::DatumDesc;
    use mudu_type::dat_type::DatType;
    use mudu_type::dat_type_id::DatTypeID;

    fn i32_desc() -> DatumDesc {
        DatumDesc::new("x".to_string(), DatType::new_no_param(DatTypeID::I32))
    }

    fn string_desc() -> DatumDesc {
        DatumDesc::new("s".to_string(), DatType::default_for(DatTypeID::String))
    }

    #[test]
    fn datum_to_binary_and_from_binary_roundtrip_i32() {
        let desc = i32_desc();
        let original = 42i32;
        let binary = datum_to_binary(&original, &desc).unwrap();
        let restored: i32 = datum_from_binary(&binary, &desc).unwrap();
        assert_eq!(restored, original);
    }

    #[test]
    fn datum_to_binary_and_from_binary_roundtrip_string() {
        let desc = string_desc();
        let original = "hello".to_string();
        let binary = datum_to_binary(&original, &desc).unwrap();
        let restored: String = datum_from_binary(&binary, &desc).unwrap();
        assert_eq!(restored, original);
    }

    #[test]
    fn datum_to_value_and_from_value_roundtrip_i32() {
        let ty = DatType::new_no_param(DatTypeID::I32);
        let original = 42i32;
        let value = datum_to_value(&original, &ty).unwrap();
        let restored: i32 = datum_from_value(&value).unwrap();
        assert_eq!(restored, original);
    }

    #[test]
    fn datum_to_value_and_from_value_roundtrip_string() {
        let ty = DatType::default_for(DatTypeID::String);
        let original = "hello".to_string();
        let value = datum_to_value(&original, &ty).unwrap();
        let restored: String = datum_from_value(&value).unwrap();
        assert_eq!(restored, original);
    }

    #[test]
    fn datum_to_value_i64_produces_expected_dat_value() {
        let ty = DatType::new_no_param(DatTypeID::I64);
        let value = datum_to_value(&123456789i64, &ty).unwrap();
        assert_eq!(*value.as_i64().unwrap(), 123456789i64);
    }

    #[test]
    fn datum_from_value_extracts_f64() {
        let value = mudu_type::dat_value::DatValue::from_f64(2.5);
        let restored: f64 = datum_from_value(&value).unwrap();
        assert!((restored - 2.5).abs() < f64::EPSILON);
    }
}
