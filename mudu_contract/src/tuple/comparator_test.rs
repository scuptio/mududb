#[cfg(test)]
#[allow(clippy::unwrap_used)]
#[allow(clippy::expect_used)]
#[allow(clippy::panic)]
mod tests {
    use crate::tuple::build_tuple::build_tuple;
    use crate::tuple::comparator::{
        TupleComparator, tuple_compare, tuple_equal, tuple_hash, tuple_hash_finish,
    };
    use crate::tuple::datum_convert::datum_to_binary;
    use crate::tuple::datum_desc::DatumDesc;
    use crate::tuple::slot::Slot;
    use crate::tuple::tuple_binary_desc::TupleBinaryDesc;
    use mudu::common::buf::Buf;
    use mudu::error::ErrorCode;
    use mudu_type::data_type::DataType;
    use mudu_type::type_family::TypeFamily;
    use std::cmp::Ordering;
    use std::collections::hash_map::DefaultHasher;
    use std::hash::Hasher;

    fn i32_desc() -> TupleBinaryDesc {
        TupleBinaryDesc::from(vec![DataType::new_no_param(TypeFamily::I32)]).unwrap()
    }

    fn make_i32_tuple(value: i32) -> Buf {
        let binary = datum_to_binary(
            &value,
            &DatumDesc::new("x".to_string(), DataType::new_no_param(TypeFamily::I32)),
        )
        .unwrap();
        build_tuple(&[binary], &i32_desc()).unwrap()
    }

    fn mixed_desc() -> TupleBinaryDesc {
        // Normalized order is fixed-length types before variable-length types.
        TupleBinaryDesc::from(vec![
            DataType::new_no_param(TypeFamily::I32),
            DataType::default_for(TypeFamily::String),
        ])
        .unwrap()
    }

    fn make_mixed_tuple(i: i32, s: &str) -> Buf {
        let i_binary = datum_to_binary(
            &i,
            &DatumDesc::new("i".to_string(), DataType::new_no_param(TypeFamily::I32)),
        )
        .unwrap();
        let s_binary = datum_to_binary(
            &s.to_string(),
            &DatumDesc::new("s".to_string(), DataType::default_for(TypeFamily::String)),
        )
        .unwrap();
        build_tuple(&[i_binary, s_binary], &mixed_desc()).unwrap()
    }

    #[test]
    fn tuple_comparator_new_and_default() {
        let comparator = TupleComparator::new();
        let a = make_i32_tuple(1);
        let b = make_i32_tuple(2);
        assert_eq!(
            (comparator.compare)(&a, &b, &i32_desc()).unwrap(),
            Ordering::Less
        );

        let default = TupleComparator::default();
        assert_eq!(
            (default.compare)(&a, &b, &i32_desc()).unwrap(),
            Ordering::Less
        );
    }

    #[test]
    fn tuple_compare_equal() {
        let a = make_i32_tuple(42);
        let b = make_i32_tuple(42);
        assert_eq!(tuple_compare(&i32_desc(), &a, &b).unwrap(), Ordering::Equal);
    }

    #[test]
    fn tuple_compare_less() {
        let small = make_i32_tuple(1);
        let large = make_i32_tuple(100);
        assert_eq!(
            tuple_compare(&i32_desc(), &small, &large).unwrap(),
            Ordering::Less
        );
    }

    #[test]
    fn tuple_compare_greater() {
        let small = make_i32_tuple(1);
        let large = make_i32_tuple(100);
        assert_eq!(
            tuple_compare(&i32_desc(), &large, &small).unwrap(),
            Ordering::Greater
        );
    }

    #[test]
    fn tuple_compare_mixed_fields() {
        let a = make_mixed_tuple(1, "a");
        let b = make_mixed_tuple(1, "b");
        let c = make_mixed_tuple(2, "a");
        assert_eq!(
            tuple_compare(&mixed_desc(), &a, &b).unwrap(),
            Ordering::Less
        );
        assert_eq!(
            tuple_compare(&mixed_desc(), &b, &a).unwrap(),
            Ordering::Greater
        );
        assert_eq!(
            tuple_compare(&mixed_desc(), &a, &c).unwrap(),
            Ordering::Less
        );
    }

    #[test]
    fn tuple_equal_equal() {
        let a = make_i32_tuple(7);
        let b = make_i32_tuple(7);
        assert!(tuple_equal(&i32_desc(), &a, &b).unwrap());
    }

    #[test]
    fn tuple_equal_not_equal() {
        let a = make_i32_tuple(7);
        let b = make_i32_tuple(9);
        assert!(!tuple_equal(&i32_desc(), &a, &b).unwrap());
    }

    #[test]
    fn tuple_equal_mixed_fields() {
        let a = make_mixed_tuple(1, "x");
        let b = make_mixed_tuple(1, "x");
        let c = make_mixed_tuple(1, "y");
        assert!(tuple_equal(&mixed_desc(), &a, &b).unwrap());
        assert!(!tuple_equal(&mixed_desc(), &a, &c).unwrap());
    }

    #[test]
    fn tuple_hash_consistent_with_equality() {
        let a = make_i32_tuple(42);
        let b = make_i32_tuple(42);
        let c = make_i32_tuple(7);

        let mut hasher_a = DefaultHasher::new();
        let mut hasher_b = DefaultHasher::new();
        let mut hasher_c = DefaultHasher::new();

        tuple_hash(&i32_desc(), &a, &mut hasher_a).unwrap();
        tuple_hash(&i32_desc(), &b, &mut hasher_b).unwrap();
        tuple_hash(&i32_desc(), &c, &mut hasher_c).unwrap();

        assert_eq!(hasher_a.finish(), hasher_b.finish());
        assert_ne!(hasher_a.finish(), hasher_c.finish());
    }

    #[test]
    fn tuple_hash_finish_matches_manual_finish() {
        let tuple = make_i32_tuple(123);
        let mut hasher = DefaultHasher::new();
        tuple_hash(&i32_desc(), &tuple, &mut hasher).unwrap();
        let expected = hasher.finish();

        let mut hasher = DefaultHasher::new();
        let actual = tuple_hash_finish(&i32_desc(), &tuple, &mut hasher).unwrap();
        assert_eq!(actual, expected);
    }

    #[test]
    fn tuple_hash_mixed_fields() {
        let a = make_mixed_tuple(1, "hello");
        let b = make_mixed_tuple(1, "hello");
        let c = make_mixed_tuple(1, "world");

        let mut hasher_a = DefaultHasher::new();
        let mut hasher_b = DefaultHasher::new();
        let mut hasher_c = DefaultHasher::new();

        tuple_hash(&mixed_desc(), &a, &mut hasher_a).unwrap();
        tuple_hash(&mixed_desc(), &b, &mut hasher_b).unwrap();
        tuple_hash(&mixed_desc(), &c, &mut hasher_c).unwrap();

        assert_eq!(hasher_a.finish(), hasher_b.finish());
        assert_ne!(hasher_a.finish(), hasher_c.finish());
    }

    #[test]
    fn tuple_compare_rejects_truncated_tuple() {
        let full = make_i32_tuple(42);
        let truncated = &full[..2];
        let err = tuple_compare(&i32_desc(), truncated, &full).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::IndexOutOfRange);
    }

    #[test]
    fn tuple_equal_rejects_truncated_tuple() {
        let full = make_i32_tuple(42);
        let truncated = &full[..2];
        let err = tuple_equal(&i32_desc(), truncated, &full).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::IndexOutOfRange);
    }

    #[test]
    fn tuple_hash_rejects_truncated_tuple() {
        let full = make_i32_tuple(42);
        let truncated = &full[..2];
        let mut hasher = DefaultHasher::new();
        let err = tuple_hash(&i32_desc(), truncated, &mut hasher).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::IndexOutOfRange);
    }

    #[test]
    fn tuple_compare_var_len_rejects_truncated_tuple() {
        let full = make_mixed_tuple(1, "hello");
        // Truncate within the fixed portion so the var slot is unreadable.
        let truncated = &full[..full.len() - 2];
        let err = tuple_compare(&mixed_desc(), &full, truncated).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::IndexOutOfRange);
    }

    #[test]
    fn tuple_comparator_adapter_functions_work() {
        let cmp = TupleComparator::new();
        let a = make_i32_tuple(1);
        let b = make_i32_tuple(2);

        assert_eq!((cmp.compare)(&a, &b, &i32_desc()).unwrap(), Ordering::Less);
        assert!(!(cmp.equal)(&a, &b, &i32_desc()).unwrap());

        let mut hasher = DefaultHasher::new();
        (cmp.hash_cal_one)(&a, &i32_desc(), &mut hasher).unwrap();
        let manual = hasher.finish();

        let mut hasher = DefaultHasher::new();
        let finish = (cmp.hash_cal_finish)(&a, &i32_desc(), &mut hasher).unwrap();
        assert_eq!(finish, manual);
    }

    fn f32_desc() -> TupleBinaryDesc {
        TupleBinaryDesc::from(vec![DataType::new_no_param(TypeFamily::F32)]).unwrap()
    }

    fn make_f32_tuple(value: f32) -> Buf {
        let binary = datum_to_binary(
            &value,
            &DatumDesc::new("x".to_string(), DataType::new_no_param(TypeFamily::F32)),
        )
        .unwrap();
        build_tuple(&[binary], &f32_desc()).unwrap()
    }

    #[test]
    fn tuple_compare_unsupported_type_returns_error() {
        let a = make_f32_tuple(1.0);
        let err = tuple_compare(&f32_desc(), &a, &a).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::UnsupportedOperation);
    }

    #[test]
    fn tuple_equal_unsupported_type_returns_error() {
        let a = make_f32_tuple(1.0);
        let err = tuple_equal(&f32_desc(), &a, &a).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::UnsupportedOperation);
    }

    #[test]
    fn tuple_hash_unsupported_type_returns_error() {
        let a = make_f32_tuple(1.0);
        let mut hasher = DefaultHasher::new();
        let err = tuple_hash(&f32_desc(), &a, &mut hasher).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::InvalidTuple);
    }

    #[test]
    fn tuple_hash_reports_conversion_error_on_bad_bytes() {
        let mut tuple = make_mixed_tuple(1, "hello").to_vec();
        let desc = mixed_desc();
        let var_fd = desc.var_len_field_desc().first().unwrap();
        let slot = Slot::from_binary(&tuple[var_fd.slot().offset()..]).unwrap();
        tuple[slot.offset()..slot.offset() + slot.length()].fill(0xff);

        let mut hasher = DefaultHasher::new();
        let err = tuple_hash(&desc, &tuple, &mut hasher).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::TypeConversionFailed);
    }

    #[test]
    fn tuple_compare_reports_invalid_tuple_on_bad_var_bytes() {
        let mut tuple = make_mixed_tuple(1, "hello").to_vec();
        let desc = mixed_desc();
        let var_fd = desc.var_len_field_desc().first().unwrap();
        let slot = Slot::from_binary(&tuple[var_fd.slot().offset()..]).unwrap();
        tuple[slot.offset()..slot.offset() + slot.length()].fill(0xff);

        let other = make_mixed_tuple(1, "hello");
        let err = tuple_compare(&desc, &tuple, &other).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::InvalidTuple);
    }
}
