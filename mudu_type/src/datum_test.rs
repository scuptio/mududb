#[cfg(test)]
#[allow(clippy::borrowed_box)]
mod tests {
    use crate::array::new_array_type;
    use crate::data_type::DataType;
    use crate::datum::{
        AsDatumDynRef, Datum, DatumDyn, binary_from_typed, binary_to_typed, value_from_typed,
        value_to_typed,
    };
    use crate::type_family::TypeFamily;
    use mudu::data_type::date::DateValue;
    use mudu::data_type::numeric::Numeric;
    use mudu::data_type::time::TimeValue;
    use mudu::data_type::timestamp::TimestampValue;
    use mudu::data_type::timestamptz::TimestampTzValue;
    use mudu::error::ErrorCode;

    #[test]
    fn vec_i32_datum_type_is_array() {
        assert_eq!(Vec::<i32>::data_type().type_family(), TypeFamily::Array);
    }

    #[test]
    fn vec_i32_to_value_roundtrip() {
        let arr = vec![1i32, 2, 3];
        let array_type = Vec::<i32>::data_type();
        let value = arr.to_value(&array_type).unwrap();
        let back = Vec::<i32>::from_value(&value).unwrap();
        assert_eq!(back, arr);
    }

    #[test]
    fn vec_i32_to_binary_roundtrip() {
        let arr = vec![1i32, 2, 3];
        let array_type = Vec::<i32>::data_type();
        let binary = arr.to_binary(&array_type).unwrap();
        let back = Vec::<i32>::from_binary(binary.as_ref()).unwrap();
        assert_eq!(back, arr);
    }

    #[test]
    fn vec_i32_to_textual_roundtrip() {
        let arr = vec![1i32, 2, 3];
        let array_type = Vec::<i32>::data_type();
        let textual = arr.to_textual(&array_type).unwrap();
        let back = Vec::<i32>::from_textual(textual.as_ref()).unwrap();
        assert_eq!(back, arr);
    }

    #[test]
    fn vec_i32_datum_dyn_methods_with_array_type() {
        let arr: Vec<i32> = vec![10, 20];
        let array_type = Vec::<i32>::data_type();

        assert_eq!(arr.type_family().unwrap(), TypeFamily::Array);

        let value = DatumDyn::to_value(&arr, &array_type).unwrap();
        assert!(value.as_array().is_some());

        let binary = DatumDyn::to_binary(&arr, &array_type).unwrap();
        assert!(!binary.as_ref().is_empty());

        let textual = DatumDyn::to_textual(&arr, &array_type).unwrap();
        assert!(!textual.as_ref().is_empty());
    }

    #[test]
    fn vec_i32_datum_dyn_methods_reject_non_array_type() {
        let arr: Vec<i32> = vec![10, 20];
        let i32_type = DataType::new_no_param(TypeFamily::I32);

        assert!(DatumDyn::to_value(&arr, &i32_type).is_err());
        assert!(DatumDyn::to_binary(&arr, &i32_type).is_err());
        assert!(DatumDyn::to_textual(&arr, &i32_type).is_err());
    }

    #[test]
    fn vec_i32_clone_boxed_produces_equivalent_dyn() {
        let arr: Vec<i32> = vec![7, 8];
        let array_type = Vec::<i32>::data_type();
        let cloned = arr.clone_boxed();

        assert_eq!(cloned.type_family().unwrap(), TypeFamily::Array);
        let original_value = arr.to_value(&array_type).unwrap();
        let cloned_value = cloned.to_value(&array_type).unwrap();
        assert_eq!(
            original_value.expect_array().len(),
            cloned_value.expect_array().len()
        );
    }

    #[test]
    fn vec_string_to_value_roundtrip() {
        let arr = vec!["a".to_string(), "b".to_string()];
        let array_type = Vec::<String>::data_type();
        let value = arr.to_value(&array_type).unwrap();
        let back = Vec::<String>::from_value(&value).unwrap();
        assert_eq!(back, arr);
    }

    #[test]
    fn as_datum_dyn_ref_for_box_dyn() {
        let boxed: Box<dyn DatumDyn> = Box::new(42i32);
        let dyn_ref = boxed.as_datum_dyn_ref();
        assert_eq!(dyn_ref.type_family().unwrap(), TypeFamily::I32);
    }

    #[test]
    #[allow(clippy::borrowed_box)]
    fn as_datum_dyn_ref_for_reference_to_box_dyn() {
        let boxed: Box<dyn DatumDyn> = Box::new(42i32);
        let reference: &Box<dyn DatumDyn> = &boxed;
        let dyn_ref = reference.as_datum_dyn_ref();
        assert_eq!(dyn_ref.type_family().unwrap(), TypeFamily::I32);
    }

    #[test]
    fn as_datum_dyn_ref_for_slice_uses_first_element() {
        let slice: &[Box<dyn DatumDyn>] = &[Box::new(42i32), Box::new(43i32)];
        let dyn_ref = slice.as_datum_dyn_ref();
        assert_eq!(dyn_ref.type_family().unwrap(), TypeFamily::I32);
    }

    #[test]
    fn as_datum_dyn_ref_for_vec_uses_first_element() {
        let vec: Vec<Box<dyn DatumDyn>> = vec![Box::new(42i64), Box::new(43i64)];
        let dyn_ref = vec.as_datum_dyn_ref();
        assert_eq!(dyn_ref.type_family().unwrap(), TypeFamily::I64);
    }

    #[test]
    fn as_datum_dyn_ref_for_fixed_array_uses_first_element() {
        let arr: [Box<dyn DatumDyn>; 2] = [Box::new(42i64), Box::new(43i64)];
        let dyn_ref = arr.as_datum_dyn_ref();
        assert_eq!(dyn_ref.type_family().unwrap(), TypeFamily::I64);
    }

    #[test]
    #[should_panic(expected = "Empty slice")]
    fn as_datum_dyn_ref_panics_on_empty_slice() {
        let empty: &[Box<dyn DatumDyn>] = &[];
        let _ = empty.as_datum_dyn_ref();
    }

    #[test]
    #[should_panic(expected = "Empty vector")]
    fn as_datum_dyn_ref_panics_on_empty_vec() {
        let empty: Vec<Box<dyn DatumDyn>> = Vec::new();
        let _ = empty.as_datum_dyn_ref();
    }

    #[test]
    #[should_panic(expected = "Empty array")]
    fn as_datum_dyn_ref_panics_on_empty_fixed_array() {
        let empty: [Box<dyn DatumDyn>; 0] = [];
        let _ = empty.as_datum_dyn_ref();
    }

    #[test]
    fn from_binary_rejects_bad_input() {
        assert!(Vec::<i32>::from_binary(&[0xff; 2]).is_err());
    }

    #[test]
    fn from_textual_rejects_bad_input() {
        assert!(Vec::<i32>::from_textual("not-an-array").is_err());
    }

    #[test]
    fn new_array_type_creates_array_data_type() {
        let inner = DataType::new_no_param(TypeFamily::I64);
        let array_type = new_array_type(inner);
        assert_eq!(array_type.type_family(), TypeFamily::Array);
    }

    #[test]
    fn empty_vec_i32_roundtrips_through_value_and_binary() {
        let arr: Vec<i32> = vec![];
        let array_type = Vec::<i32>::data_type();

        let value = arr.to_value(&array_type).unwrap();
        assert!(value.as_array().unwrap().is_empty());
        assert_eq!(Vec::<i32>::from_value(&value).unwrap(), arr);

        let binary = arr.to_binary(&array_type).unwrap();
        assert_eq!(Vec::<i32>::from_binary(binary.as_ref()).unwrap(), arr);
    }

    #[test]
    fn empty_vec_string_roundtrips_through_value() {
        let arr: Vec<String> = vec![];
        let array_type = Vec::<String>::data_type();
        let value = arr.to_value(&array_type).unwrap();
        assert_eq!(Vec::<String>::from_value(&value).unwrap(), arr);
    }

    macro_rules! scalar_datum_tests {
        ($name:ident, $type:ty, $variant:ident, $value:expr, $wrong_type:ident) => {
            paste::paste! {
                #[test]
                fn [<scalar_ $name _type_family>]() {
                    let typed_datum: $type = $value;
                    assert_eq!(
                        DatumDyn::type_family(&typed_datum).unwrap(),
                        TypeFamily::$variant
                    );
                }

                #[test]
                fn [<scalar_ $name _to_value_roundtrip>]() {
                    let typed_datum: $type = $value;
                    let data_type = <$type as Datum>::data_type();
                    let value = DatumDyn::to_value(&typed_datum, &data_type).unwrap();
                    let back = <$type as Datum>::from_value(&value).unwrap();
                    assert_eq!(back, typed_datum);
                }

                #[test]
                fn [<scalar_ $name _to_binary_roundtrip>]() {
                    let typed_datum: $type = $value;
                    let data_type = <$type as Datum>::data_type();
                    let binary = DatumDyn::to_binary(&typed_datum, &data_type).unwrap();
                    let back = <$type as Datum>::from_binary(binary.as_ref()).unwrap();
                    assert_eq!(back, typed_datum);
                }

                #[test]
                fn [<scalar_ $name _to_textual_roundtrip>]() {
                    let typed_datum: $type = $value;
                    let data_type = <$type as Datum>::data_type();
                    let textual = DatumDyn::to_textual(&typed_datum, &data_type).unwrap();
                    let back = <$type as Datum>::from_textual(textual.as_ref()).unwrap();
                    assert_eq!(back, typed_datum);
                }

                #[test]
                fn [<scalar_ $name _rejects_wrong_type_for_to_value>]() {
                    let typed_datum: $type = $value;
                    let wrong_type = DataType::default_for(TypeFamily::$wrong_type);
                    let err = DatumDyn::to_value(&typed_datum, &wrong_type).err().unwrap();
                    assert_eq!(err.ec(), ErrorCode::InvalidType);
                }

                #[test]
                fn [<scalar_ $name _rejects_wrong_type_for_to_binary>]() {
                    let typed_datum: $type = $value;
                    let wrong_type = DataType::default_for(TypeFamily::$wrong_type);
                    let err = DatumDyn::to_binary(&typed_datum, &wrong_type).err().unwrap();
                    assert_eq!(err.ec(), ErrorCode::InvalidType);
                }

                #[test]
                fn [<scalar_ $name _rejects_wrong_type_for_to_textual>]() {
                    let typed_datum: $type = $value;
                    let wrong_type = DataType::default_for(TypeFamily::$wrong_type);
                    let err = DatumDyn::to_textual(&typed_datum, &wrong_type).err().unwrap();
                    assert_eq!(err.ec(), ErrorCode::InvalidType);
                }

                #[test]
                fn [<scalar_ $name _clone_boxed_is_equivalent>]() {
                    let typed_datum: $type = $value;
                    let data_type = <$type as Datum>::data_type();
                    let cloned: Box<dyn DatumDyn> = typed_datum.clone_boxed();
                    assert_eq!(cloned.type_family().unwrap(), TypeFamily::$variant);
                    let cloned_value = cloned.to_value(&data_type).unwrap();
                    let back = <$type as Datum>::from_value(&cloned_value).unwrap();
                    assert_eq!(back, typed_datum);
                }
            }
        };
    }

    scalar_datum_tests!(i32, i32, I32, 42i32, I64);
    scalar_datum_tests!(i64, i64, I64, 99i64, I32);
    scalar_datum_tests!(i128, i128, I128, 1701411834604692317i128, I64);
    scalar_datum_tests!(u128, u128, U128, 12345u128, I64);
    scalar_datum_tests!(f32, f32, F32, 1.5f32, F64);
    scalar_datum_tests!(f64, f64, F64, 2.5f64, F32);
    scalar_datum_tests!(string, String, String, "hello".to_string(), I32);

    #[test]
    fn scalar_numeric_roundtrips() {
        let numeric = Numeric::parse("123").unwrap();
        let data_type = Numeric::data_type();

        let value = DatumDyn::to_value(&numeric, &data_type).unwrap();
        let back = Numeric::from_value(&value).unwrap();
        assert_eq!(back.to_plain_string(), numeric.to_plain_string());

        let binary = DatumDyn::to_binary(&numeric, &data_type).unwrap();
        let back = Numeric::from_binary(binary.as_ref()).unwrap();
        assert_eq!(back.to_plain_string(), numeric.to_plain_string());

        let textual = DatumDyn::to_textual(&numeric, &data_type).unwrap();
        let back = Numeric::from_textual(textual.as_ref()).unwrap();
        assert_eq!(back.to_plain_string(), numeric.to_plain_string());
    }

    #[test]
    fn scalar_numeric_rejects_wrong_type() {
        let numeric = Numeric::zero();
        let wrong_type = DataType::default_for(TypeFamily::I64);
        assert_eq!(
            DatumDyn::to_value(&numeric, &wrong_type)
                .err()
                .unwrap()
                .ec(),
            ErrorCode::InvalidType
        );
        assert_eq!(
            DatumDyn::to_binary(&numeric, &wrong_type)
                .err()
                .unwrap()
                .ec(),
            ErrorCode::InvalidType
        );
        assert_eq!(
            DatumDyn::to_textual(&numeric, &wrong_type)
                .err()
                .unwrap()
                .ec(),
            ErrorCode::InvalidType
        );
    }

    #[test]
    fn scalar_date_roundtrips() {
        let date = DateValue::parse("2026-05-20").unwrap();
        let data_type = DateValue::data_type();

        let value = DatumDyn::to_value(&date, &data_type).unwrap();
        let back = DateValue::from_value(&value).unwrap();
        assert_eq!(back.days_since_epoch(), date.days_since_epoch());

        let binary = DatumDyn::to_binary(&date, &data_type).unwrap();
        let back = DateValue::from_binary(binary.as_ref()).unwrap();
        assert_eq!(back.days_since_epoch(), date.days_since_epoch());

        let textual = DatumDyn::to_textual(&date, &data_type).unwrap();
        let back = DateValue::from_textual(textual.as_ref()).unwrap();
        assert_eq!(back.days_since_epoch(), date.days_since_epoch());
    }

    #[test]
    fn scalar_time_roundtrips() {
        let time = TimeValue::parse("12:34:56.123456").unwrap();
        let data_type = TimeValue::data_type();

        let value = DatumDyn::to_value(&time, &data_type).unwrap();
        let back = TimeValue::from_value(&value).unwrap();
        assert_eq!(back.micros_since_midnight(), time.micros_since_midnight());

        let binary = DatumDyn::to_binary(&time, &data_type).unwrap();
        let back = TimeValue::from_binary(binary.as_ref()).unwrap();
        assert_eq!(back.micros_since_midnight(), time.micros_since_midnight());
    }

    #[test]
    fn scalar_timestamp_roundtrips() {
        let ts = TimestampValue::parse("2026-05-20T14:30:45.123456").unwrap();
        let data_type = TimestampValue::data_type();

        let value = DatumDyn::to_value(&ts, &data_type).unwrap();
        let back = TimestampValue::from_value(&value).unwrap();
        assert_eq!(back.epoch_micros(), ts.epoch_micros());

        let binary = DatumDyn::to_binary(&ts, &data_type).unwrap();
        let back = TimestampValue::from_binary(binary.as_ref()).unwrap();
        assert_eq!(back.epoch_micros(), ts.epoch_micros());
    }

    #[test]
    fn scalar_timestamptz_roundtrips() {
        let tstz = TimestampTzValue::parse("2026-05-20T14:30:45.123456+08:00").unwrap();
        let data_type = TimestampTzValue::data_type();

        let value = DatumDyn::to_value(&tstz, &data_type).unwrap();
        let back = TimestampTzValue::from_value(&value).unwrap();
        assert_eq!(back.epoch_micros_utc(), tstz.epoch_micros_utc());

        let binary = DatumDyn::to_binary(&tstz, &data_type).unwrap();
        let back = TimestampTzValue::from_binary(binary.as_ref()).unwrap();
        assert_eq!(back.epoch_micros_utc(), tstz.epoch_micros_utc());
    }

    #[test]
    fn scalar_from_textual_rejects_bad_input() {
        assert!(i32::from_textual("not-a-number").is_err());
        assert!(Numeric::from_textual("not-a-number").is_err());
        assert!(DateValue::from_textual("not-a-date").is_err());
    }

    #[test]
    fn scalar_from_binary_rejects_bad_input() {
        assert!(i32::from_binary(&[0xff; 2]).is_err());
        assert!(Numeric::from_binary(&[]).is_err());
    }

    #[test]
    fn as_datum_dyn_ref_for_double_reference() {
        let boxed: Box<dyn DatumDyn> = Box::new(42i32);
        let reference: &Box<dyn DatumDyn> = &boxed;
        let double_ref: &&Box<dyn DatumDyn> = &reference;
        let dyn_ref = double_ref.as_datum_dyn_ref();
        assert_eq!(dyn_ref.type_family().unwrap(), TypeFamily::I32);
    }

    #[test]
    fn as_datum_dyn_ref_for_slice_of_references() {
        let first: Box<dyn DatumDyn> = Box::new(42i32);
        let second: Box<dyn DatumDyn> = Box::new(43i32);
        let refs: &[&Box<dyn DatumDyn>] = &[&first, &second];
        let dyn_ref = refs.as_datum_dyn_ref();
        assert_eq!(dyn_ref.type_family().unwrap(), TypeFamily::I32);
    }

    #[test]
    fn as_datum_dyn_ref_for_vec_of_references() {
        let first: Box<dyn DatumDyn> = Box::new(42i64);
        let second: Box<dyn DatumDyn> = Box::new(43i64);
        let refs: Vec<&Box<dyn DatumDyn>> = vec![&first, &second];
        let dyn_ref = refs.as_datum_dyn_ref();
        assert_eq!(dyn_ref.type_family().unwrap(), TypeFamily::I64);
    }

    #[test]
    fn typed_helpers_roundtrip_i32() {
        let value: i32 = 42;
        let type_name = "int";

        let binary = binary_from_typed(&value, type_name).unwrap();
        let back: i32 = binary_to_typed(binary.as_ref(), type_name).unwrap();
        assert_eq!(back, value);

        let data_value = value_from_typed(&value, type_name).unwrap();
        let back: i32 = value_to_typed(&data_value, type_name).unwrap();
        assert_eq!(back, value);
    }

    #[test]
    fn typed_helpers_roundtrip_string() {
        let value = "hello".to_string();
        let type_name = "varchar";

        let binary = binary_from_typed(&value, type_name).unwrap();
        let back: String = binary_to_typed(binary.as_ref(), type_name).unwrap();
        assert_eq!(back, value);

        let data_value = value_from_typed(&value, type_name).unwrap();
        let back: String = value_to_typed(&data_value, type_name).unwrap();
        assert_eq!(back, value);
    }

    #[test]
    fn typed_helpers_reject_bad_binary() {
        let err = binary_to_typed::<i32, _>(&[0xff; 2], "int").unwrap_err();
        assert_eq!(err.ec(), ErrorCode::InsufficientBufferSpace);
    }

    #[test]
    fn typed_helpers_reject_bad_textual() {
        let err = binary_to_typed::<Vec<i32>, _>(b"not-an-array", "int[]").unwrap_err();
        assert_eq!(err.ec(), ErrorCode::TypeConversionFailed);
    }
}
