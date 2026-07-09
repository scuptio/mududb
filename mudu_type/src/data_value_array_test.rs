#[cfg(test)]
mod tests {
    use crate::array::data_value_array::DataValueArray;
    use crate::data_value::DataValue;
    use crate::type_family::TypeFamily;

    #[test]
    fn i32_array_constructor_getter_and_type_id() {
        let arr = DataValueArray::from_i32(vec![1, 2, 3]);
        assert_eq!(arr.get_type_family(), TypeFamily::I32);
        assert_eq!(arr.as_i32(), Some(&vec![1, 2, 3]));
        assert_eq!(arr.expect_i32(), &vec![1, 2, 3]);
        assert!(arr.as_i64().is_none());
        assert!(format!("{:?}", arr).contains("I32"));
    }

    #[test]
    fn i64_array_constructor_getter_and_type_id() {
        let arr = DataValueArray::from_i64(vec![10, 20]);
        assert_eq!(arr.get_type_family(), TypeFamily::I64);
        assert_eq!(arr.as_i64(), Some(&vec![10, 20]));
        assert_eq!(arr.expect_i64(), &vec![10, 20]);
        assert!(arr.as_i32().is_none());
    }

    #[test]
    fn f32_array_constructor_getter_and_type_id() {
        let arr = DataValueArray::from_f32(vec![1.5, 2.5]);
        assert_eq!(arr.get_type_family(), TypeFamily::F32);
        assert_eq!(arr.as_f32(), Some(&vec![1.5, 2.5]));
        assert_eq!(arr.expect_f32(), &vec![1.5, 2.5]);
        assert!(arr.as_f64().is_none());
    }

    #[test]
    fn f64_array_constructor_getter_and_type_id() {
        let arr = DataValueArray::from_f64(vec![3.5, 2.25]);
        assert_eq!(arr.get_type_family(), TypeFamily::F64);
        assert_eq!(arr.as_f64(), Some(&vec![3.5, 2.25]));
        assert_eq!(arr.expect_f64(), &vec![3.5, 2.25]);
        assert!(arr.as_f32().is_none());
    }

    #[test]
    fn string_array_constructor_getter_and_type_id() {
        let arr = DataValueArray::from_string(vec!["a".to_string(), "b".to_string()]);
        assert_eq!(arr.get_type_family(), TypeFamily::String);
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
            DataValue::from_i32(1),
            DataValue::from_string("x".to_string()),
        ]];
        let arr = DataValueArray::from_object(rows);
        assert_eq!(arr.get_type_family(), TypeFamily::Record);
        assert!(arr.as_object().is_some());
        assert_eq!(arr.as_object().unwrap().len(), 1);
        assert_eq!(arr.as_object().unwrap()[0].len(), 2);
        assert_eq!(arr.expect_object().len(), 1);
        assert!(arr.as_array().is_none());
    }

    #[test]
    fn nested_array_constructor_getter_and_type_id() {
        let nested = vec![DataValueArray::from_i32(vec![1, 2])];
        let arr = DataValueArray::from_array(vec![nested]);
        assert_eq!(arr.get_type_family(), TypeFamily::Array);
        assert!(arr.as_array().is_some());
        assert_eq!(arr.as_array().unwrap().len(), 1);
        assert_eq!(arr.as_array().unwrap()[0].len(), 1);
        assert_eq!(arr.expect_array().len(), 1);
        assert!(arr.as_object().is_none());
    }

    #[test]
    fn clone_preserves_all_variants() {
        let cases: Vec<(DataValueArray, TypeFamily)> = vec![
            (DataValueArray::from_i32(vec![1]), TypeFamily::I32),
            (DataValueArray::from_i64(vec![2]), TypeFamily::I64),
            (DataValueArray::from_f32(vec![1.0]), TypeFamily::F32),
            (DataValueArray::from_f64(vec![2.0]), TypeFamily::F64),
            (
                DataValueArray::from_string(vec!["s".to_string()]),
                TypeFamily::String,
            ),
            (
                DataValueArray::from_object(vec![vec![DataValue::from_i32(1)]]),
                TypeFamily::Record,
            ),
            (
                DataValueArray::from_array(vec![vec![DataValueArray::from_i32(vec![1])]]),
                TypeFamily::Array,
            ),
        ];

        for (arr, expected_id) in cases {
            let cloned = arr.clone();
            assert_eq!(cloned.get_type_family(), expected_id);
            assert_eq!(format!("{:?}", arr), format!("{:?}", cloned));
        }
    }

    #[test]
    fn debug_output_contains_variant_name_and_payload() {
        let arr = DataValueArray::from_i32(vec![7, 8]);
        let debug = format!("{:?}", arr);
        assert!(debug.starts_with("I32("));
        assert!(debug.contains("7"));
        assert!(debug.contains("8"));
    }
}
