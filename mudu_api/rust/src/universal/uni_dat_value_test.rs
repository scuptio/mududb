#[cfg(test)]
mod tests {
    #![allow(clippy::panic)]

    use crate::universal::uni_dat_value::UniDatValue;
    use crate::universal::uni_scalar_value::UniScalarValue;

    fn scalar() -> UniDatValue {
        UniDatValue::from_scalar(UniScalarValue::Bool(true))
    }

    fn array() -> UniDatValue {
        UniDatValue::from_array(vec![scalar()])
    }

    fn record() -> UniDatValue {
        UniDatValue::from_record(vec![scalar()])
    }

    fn binary() -> UniDatValue {
        UniDatValue::from_binary(vec![1, 2, 3])
    }

    #[test]
    fn default_is_scalar_false() {
        assert!(matches!(
            UniDatValue::default(),
            UniDatValue::Scalar(UniScalarValue::Bool(false))
        ));
    }

    #[test]
    fn scalar_constructor_and_accessors() {
        let v = scalar();
        assert!(matches!(v.as_scalar(), Some(UniScalarValue::Bool(true))));
        assert!(matches!(v.expect_scalar(), UniScalarValue::Bool(true)));

        assert!(array().as_scalar().is_none());
        assert!(record().as_scalar().is_none());
        assert!(binary().as_scalar().is_none());
    }

    #[test]
    fn array_constructor_and_accessors() {
        let v = array();
        assert_eq!(v.as_array().unwrap().len(), 1);
        assert_eq!(v.expect_array().len(), 1);

        assert!(scalar().as_array().is_none());
        assert!(record().as_array().is_none());
        assert!(binary().as_array().is_none());
    }

    #[test]
    fn record_constructor_and_accessors() {
        let v = record();
        assert_eq!(v.as_record().unwrap().len(), 1);
        assert_eq!(v.expect_record().len(), 1);

        assert!(scalar().as_record().is_none());
        assert!(array().as_record().is_none());
        assert!(binary().as_record().is_none());
    }

    #[test]
    fn binary_constructor_and_accessors() {
        let v = binary();
        assert_eq!(v.as_binary(), Some(&vec![1, 2, 3]));
        assert_eq!(v.expect_binary(), &vec![1, 2, 3]);

        assert!(scalar().as_binary().is_none());
        assert!(array().as_binary().is_none());
        assert!(record().as_binary().is_none());
    }

    #[test]
    fn expect_scalar_panics_on_wrong_variant() {
        let v = array();
        let err = std::panic::catch_unwind(|| {
            let _ = v.expect_scalar();
        });
        assert!(err.is_err());
    }

    #[test]
    fn expect_array_panics_on_wrong_variant() {
        let v = scalar();
        let err = std::panic::catch_unwind(|| {
            let _ = v.expect_array();
        });
        assert!(err.is_err());
    }

    #[test]
    fn expect_record_panics_on_wrong_variant() {
        let v = scalar();
        let err = std::panic::catch_unwind(|| {
            let _ = v.expect_record();
        });
        assert!(err.is_err());
    }

    #[test]
    fn expect_binary_panics_on_wrong_variant() {
        let v = scalar();
        let err = std::panic::catch_unwind(|| {
            let _ = v.expect_binary();
        });
        assert!(err.is_err());
    }

    #[test]
    fn serialize_roundtrips_through_json() {
        let values = vec![scalar(), array(), record(), binary()];
        for value in values {
            let json = serde_json::to_string(&value).unwrap();
            let decoded: UniDatValue = serde_json::from_str(&json).unwrap();
            assert_same_value(&decoded, &value);
        }
    }

    #[test]
    fn deserialize_rejects_empty_sequence() {
        let json = "[]";
        let result: Result<UniDatValue, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }

    #[test]
    fn deserialize_rejects_unknown_variant_id() {
        // [99, []] is not a known variant id.
        let json = "[99, []]";
        let result: Result<UniDatValue, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }

    #[test]
    fn deserialize_rejects_missing_scalar_value() {
        let result: Result<UniDatValue, _> = serde_json::from_str("[0]");
        assert!(result.is_err());
    }

    #[test]
    fn deserialize_rejects_missing_array_value() {
        let result: Result<UniDatValue, _> = serde_json::from_str("[1]");
        assert!(result.is_err());
    }

    #[test]
    fn deserialize_rejects_missing_record_value() {
        let result: Result<UniDatValue, _> = serde_json::from_str("[2]");
        assert!(result.is_err());
    }

    #[test]
    fn deserialize_rejects_missing_binary_value() {
        let result: Result<UniDatValue, _> = serde_json::from_str("[3]");
        assert!(result.is_err());
    }

    fn assert_same_value(actual: &UniDatValue, expected: &UniDatValue) {
        match (actual, expected) {
            (UniDatValue::Scalar(a), UniDatValue::Scalar(b)) => {
                assert!(
                    matches!((a, b), (UniScalarValue::Bool(a), UniScalarValue::Bool(b)) if a == b)
                );
            }
            (UniDatValue::Array(a), UniDatValue::Array(b)) => {
                assert_eq!(a.len(), b.len());
                for (x, y) in a.iter().zip(b.iter()) {
                    assert_same_value(x, y);
                }
            }
            (UniDatValue::Record(a), UniDatValue::Record(b)) => {
                assert_eq!(a.len(), b.len());
                for (x, y) in a.iter().zip(b.iter()) {
                    assert_same_value(x, y);
                }
            }
            (UniDatValue::Binary(a), UniDatValue::Binary(b)) => assert_eq!(a, b),
            _ => panic!("variant mismatch"),
        }
    }
}
