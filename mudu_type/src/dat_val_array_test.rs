#[cfg(test)]
mod tests {
    use crate::array::dat_val_array::DatValArray;
    use crate::dat_type_id::DatTypeID;
    use crate::dat_value::DatValue;

    #[test]
    fn i32_array_constructor_getter_and_type_id() {
        let arr = DatValArray::from_i32(vec![1, 2, 3]);
        assert_eq!(arr.get_dat_type_id(), DatTypeID::I32);
        assert_eq!(arr.as_i32(), Some(&vec![1, 2, 3]));
        assert_eq!(arr.expect_i32(), &vec![1, 2, 3]);
        assert!(arr.as_i64().is_none());
        assert!(format!("{:?}", arr).contains("I32"));
    }

    #[test]
    fn i64_array_constructor_getter_and_type_id() {
        let arr = DatValArray::from_i64(vec![10, 20]);
        assert_eq!(arr.get_dat_type_id(), DatTypeID::I64);
        assert_eq!(arr.as_i64(), Some(&vec![10, 20]));
        assert_eq!(arr.expect_i64(), &vec![10, 20]);
        assert!(arr.as_i32().is_none());
    }

    #[test]
    fn f32_array_constructor_getter_and_type_id() {
        let arr = DatValArray::from_f32(vec![1.5, 2.5]);
        assert_eq!(arr.get_dat_type_id(), DatTypeID::F32);
        assert_eq!(arr.as_f32(), Some(&vec![1.5, 2.5]));
        assert_eq!(arr.expect_f32(), &vec![1.5, 2.5]);
        assert!(arr.as_f64().is_none());
    }

    #[test]
    fn f64_array_constructor_getter_and_type_id() {
        let arr = DatValArray::from_f64(vec![3.5, 2.25]);
        assert_eq!(arr.get_dat_type_id(), DatTypeID::F64);
        assert_eq!(arr.as_f64(), Some(&vec![3.5, 2.25]));
        assert_eq!(arr.expect_f64(), &vec![3.5, 2.25]);
        assert!(arr.as_f32().is_none());
    }

    #[test]
    fn string_array_constructor_getter_and_type_id() {
        let arr = DatValArray::from_string(vec!["a".to_string(), "b".to_string()]);
        assert_eq!(arr.get_dat_type_id(), DatTypeID::String);
        assert_eq!(
            arr.as_string(),
            Some(&vec!["a".to_string(), "b".to_string()])
        );
        assert_eq!(arr.expect_string(), &vec!["a".to_string(), "b".to_string()]);
        assert!(arr.as_i32().is_none());
    }

    #[test]
    fn record_array_constructor_getter_and_type_id() {
        let rows = vec![vec![
            DatValue::from_i32(1),
            DatValue::from_string("x".to_string()),
        ]];
        let arr = DatValArray::from_object(rows);
        assert_eq!(arr.get_dat_type_id(), DatTypeID::Record);
        assert!(arr.as_object().is_some());
        assert_eq!(arr.as_object().unwrap().len(), 1);
        assert_eq!(arr.as_object().unwrap()[0].len(), 2);
        assert_eq!(arr.expect_object().len(), 1);
        assert!(arr.as_array().is_none());
    }

    #[test]
    fn nested_array_constructor_getter_and_type_id() {
        let nested = vec![DatValArray::from_i32(vec![1, 2])];
        let arr = DatValArray::from_array(vec![nested]);
        assert_eq!(arr.get_dat_type_id(), DatTypeID::Array);
        assert!(arr.as_array().is_some());
        assert_eq!(arr.as_array().unwrap().len(), 1);
        assert_eq!(arr.as_array().unwrap()[0].len(), 1);
        assert_eq!(arr.expect_array().len(), 1);
        assert!(arr.as_object().is_none());
    }

    #[test]
    fn clone_preserves_all_variants() {
        let cases: Vec<(DatValArray, DatTypeID)> = vec![
            (DatValArray::from_i32(vec![1]), DatTypeID::I32),
            (DatValArray::from_i64(vec![2]), DatTypeID::I64),
            (DatValArray::from_f32(vec![1.0]), DatTypeID::F32),
            (DatValArray::from_f64(vec![2.0]), DatTypeID::F64),
            (
                DatValArray::from_string(vec!["s".to_string()]),
                DatTypeID::String,
            ),
            (
                DatValArray::from_object(vec![vec![DatValue::from_i32(1)]]),
                DatTypeID::Record,
            ),
            (
                DatValArray::from_array(vec![vec![DatValArray::from_i32(vec![1])]]),
                DatTypeID::Array,
            ),
        ];

        for (arr, expected_id) in cases {
            let cloned = arr.clone();
            assert_eq!(cloned.get_dat_type_id(), expected_id);
            assert_eq!(format!("{:?}", arr), format!("{:?}", cloned));
        }
    }

    #[test]
    fn debug_output_contains_variant_name_and_payload() {
        let arr = DatValArray::from_i32(vec![7, 8]);
        let debug = format!("{:?}", arr);
        assert!(debug.starts_with("I32("));
        assert!(debug.contains("7"));
        assert!(debug.contains("8"));
    }
}
