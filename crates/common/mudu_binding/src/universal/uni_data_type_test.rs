//! Unit tests for `UniDataType` conversion and inline rewriting.

#![allow(missing_docs)]
#![allow(clippy::unwrap_used)]

use crate::universal::uni_data_type::UniDataType;
use crate::universal::uni_data_value::UniDataValue;
use crate::universal::uni_record_type::{UniRecordField, UniRecordType};
use crate::universal::uni_scalar::UniScalar;
use crate::universal::uni_scalar_value::UniScalarValue;
use mudu::error::ErrorCode;
use mudu_type::data_type::DataType;
use mudu_type::type_family::TypeFamily;

fn scalar(ty: UniScalar) -> UniDataType {
    UniDataType::from_scalar(ty)
}

fn i64_param(value: i64) -> Option<Vec<UniDataValue>> {
    Some(vec![UniDataValue::from_scalar(UniScalarValue::from_i64(
        value,
    ))])
}

#[test]
fn scalar_without_params_maps_to_default_data_type() {
    let uni = scalar(UniScalar::I32);
    let dat = uni.uni_to().unwrap();
    assert_eq!(dat.type_family(), TypeFamily::I32);
}

#[test]
fn string_with_length_param_maps_to_varchar() {
    let uni = scalar(UniScalar::String);
    let dat = uni.uni_to_with_params(i64_param(42)).unwrap();
    assert_eq!(dat.type_family(), TypeFamily::String);
    assert_eq!(dat.as_string_param().unwrap().length(), 42);
}

#[test]
fn string_with_non_i64_param_fails() {
    let uni = scalar(UniScalar::String);
    let params = Some(vec![UniDataValue::from_scalar(
        UniScalarValue::from_string("x".to_string()),
    )]);
    let err = uni.uni_to_with_params(params).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::TypeConversionFailed);
}

#[test]
fn numeric_with_precision_and_scale_maps_to_numeric() {
    let uni = scalar(UniScalar::Numeric);
    let params = Some(vec![
        UniDataValue::from_scalar(UniScalarValue::from_i64(10)),
        UniDataValue::from_scalar(UniScalarValue::from_i64(2)),
    ]);
    let dat = uni.uni_to_with_params(params).unwrap();
    assert_eq!(dat.type_family(), TypeFamily::Numeric);
    let param = dat.as_numeric_param().unwrap();
    assert_eq!(param.precision(), 10);
    assert_eq!(param.scale(), 2);
}

#[test]
fn numeric_with_negative_precision_fails() {
    let uni = scalar(UniScalar::Numeric);
    let err = uni.uni_to_with_params(i64_param(-1)).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::InvalidType);
}

#[test]
fn time_with_precision_maps_to_time() {
    let uni = scalar(UniScalar::Time);
    let dat = uni.uni_to_with_params(i64_param(3)).unwrap();
    assert_eq!(dat.type_family(), TypeFamily::Time);
    assert_eq!(dat.as_time_param().unwrap().precision(), 3);
}

#[test]
fn timestamp_with_precision_maps_to_timestamp() {
    let uni = scalar(UniScalar::Timestamp);
    let dat = uni.uni_to_with_params(i64_param(6)).unwrap();
    assert_eq!(dat.type_family(), TypeFamily::Timestamp);
    assert_eq!(dat.as_timestamp_param().unwrap().precision(), 6);
}

#[test]
fn timestamptz_with_precision_maps_to_timestamptz() {
    let uni = scalar(UniScalar::TimestampTz);
    let dat = uni.uni_to_with_params(i64_param(0)).unwrap();
    assert_eq!(dat.type_family(), TypeFamily::TimestampTz);
    assert_eq!(dat.as_timestamptz_param().unwrap().precision(), 0);
}

#[test]
fn unsupported_scalar_to_data_type_fails() {
    // A scalar without a concrete TypeFamily mapping should be rejected.
    // Char is currently not supported by `scalar.to()`.
    let uni = scalar(UniScalar::Char);
    let err = uni.uni_to().unwrap_err();
    assert_eq!(err.ec(), ErrorCode::InvalidType);
}

#[test]
fn array_uni_to_and_from_roundtrip() {
    let inner = scalar(UniScalar::I64);
    let uni = UniDataType::from_array(Box::new(inner));
    let dat = uni.uni_to().unwrap();
    assert_eq!(dat.type_family(), TypeFamily::Array);

    let back = UniDataType::uni_from(dat).unwrap();
    assert!(matches!(back, UniDataType::Array(_)));
}

#[test]
fn record_uni_to_and_from_roundtrip() {
    let uni = UniDataType::from_record(UniRecordType {
        record_name: "person".to_string(),
        record_fields: vec![
            UniRecordField {
                field_name: "id".to_string(),
                field_type: scalar(UniScalar::I32),
                field_attrs: Vec::new(),
            },
            UniRecordField {
                field_name: "name".to_string(),
                field_type: scalar(UniScalar::String),
                field_attrs: Vec::new(),
            },
        ],
    });
    let dat = uni.uni_to().unwrap();
    assert_eq!(dat.type_family(), TypeFamily::Record);

    let back = UniDataType::uni_from(dat).unwrap();
    assert!(matches!(back, UniDataType::Record(_)));
}

#[test]
fn binary_data_type_maps_to_scalar_blob() {
    // The Binary family (also what a null datum reports) converts through
    // its scalar Blob form.
    let dat = DataType::new_no_param(TypeFamily::Binary);
    let uni = UniDataType::uni_from(dat).unwrap();
    assert!(matches!(uni.as_scalar(), Some(UniScalar::Blob)));
}

#[test]
fn unsupported_uni_type_to_dat_fails() {
    let uni = UniDataType::from_tuple(vec![scalar(UniScalar::I32)]);
    let err = uni.uni_to().unwrap_err();
    assert_eq!(err.ec(), ErrorCode::InvalidType);
}

#[test]
fn rewrite_inline_for_independent_records() {
    let record_a = UniDataType::from_record(UniRecordType {
        record_name: "a".to_string(),
        record_fields: vec![UniRecordField {
            field_name: "x".to_string(),
            field_type: scalar(UniScalar::I32),
            field_attrs: Vec::new(),
        }],
    });
    let result = UniDataType::rewrite_inline(vec![record_a]).unwrap();
    assert_eq!(result.len(), 1);
    assert!(matches!(result[0], UniDataType::Record(_)));
}

#[test]
fn rewrite_inline_missing_dependency_fails() {
    let record = UniDataType::from_record(UniRecordType {
        record_name: "orphan".to_string(),
        record_fields: vec![UniRecordField {
            field_name: "other".to_string(),
            field_type: UniDataType::from_identifier("unknown".to_string()),
            field_attrs: Vec::new(),
        }],
    });
    let err = UniDataType::rewrite_inline(vec![record]).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::EntityNotFound);
}

// ---------------------------------------------------------------------------
// Serde shape tests (moved from the pre-generation inline `uni_data_type`
// tests; `as_array`/`as_option` now return the boxed payload, hence the
// `Box::as_ref` adaptations).
// ---------------------------------------------------------------------------

mod wire {
    use crate::universal::uni_data_type::UniDataType;
    use crate::universal::uni_record_type::{UniRecordField, UniRecordType};
    use crate::universal::uni_result_type::UniResultType;
    use crate::universal::uni_scalar::UniScalar;
    use mudu::common::serde_utils::{
        deserialize_from, deserialize_from_json, serialize_to_json, serialize_to_vec,
    };

    fn assert_json_and_binary_roundtrip(value: &UniDataType) {
        let json = serialize_to_json(value).unwrap();
        let binary = serialize_to_vec(value).unwrap();

        let decoded_json: UniDataType = deserialize_from_json(json.as_str()).unwrap();
        let (decoded_binary, used): (UniDataType, u64) =
            deserialize_from(binary.as_slice()).unwrap();

        let json_after = serialize_to_json(&decoded_json).unwrap();
        let binary_after = serialize_to_vec(&decoded_binary).unwrap();

        assert_eq!(json_after, json);
        assert_eq!(binary_after, binary);
        assert_eq!(used as usize, binary.len());
    }

    fn sample_record_type() -> UniRecordType {
        UniRecordType {
            record_name: "vote_record".to_string(),
            record_fields: vec![
                UniRecordField {
                    field_name: "id".to_string(),
                    field_type: UniDataType::Scalar(UniScalar::U128),
                    field_attrs: Vec::new(),
                },
                UniRecordField {
                    field_name: "name".to_string(),
                    field_type: UniDataType::Scalar(UniScalar::String),
                    field_attrs: Vec::new(),
                },
                UniRecordField {
                    field_name: "tags".to_string(),
                    field_type: UniDataType::Array(Box::new(UniDataType::Scalar(
                        UniScalar::String,
                    ))),
                    field_attrs: Vec::new(),
                },
            ],
        }
    }

    fn sample_data_type() -> UniDataType {
        UniDataType::Record(UniRecordType {
            record_name: "envelope".to_string(),
            record_fields: vec![
                UniRecordField {
                    field_name: "meta".to_string(),
                    field_type: UniDataType::Tuple(vec![
                        UniDataType::Scalar(UniScalar::U64),
                        UniDataType::Option(Box::new(UniDataType::Scalar(UniScalar::String))),
                    ]),
                    field_attrs: Vec::new(),
                },
                UniRecordField {
                    field_name: "payload".to_string(),
                    field_type: UniDataType::Result(UniResultType {
                        ok: Some(Box::new(UniDataType::Array(Box::new(UniDataType::Scalar(
                            UniScalar::I32,
                        ))))),
                        err: Some(Box::new(UniDataType::Identifier("ErrCode".to_string()))),
                    }),
                    field_attrs: Vec::new(),
                },
                UniRecordField {
                    field_name: "blob".to_string(),
                    field_type: UniDataType::Binary,
                    field_attrs: Vec::new(),
                },
            ],
        })
    }

    #[test]
    fn default_is_scalar_bool() {
        assert!(matches!(
            UniDataType::default(),
            UniDataType::Scalar(UniScalar::Bool)
        ));
    }

    #[test]
    fn constructors_accessors_and_expects() {
        let scalar = UniDataType::from_scalar(UniScalar::I32);
        assert_eq!(scalar.as_scalar(), Some(&UniScalar::I32));
        assert!(scalar.as_array().is_none());
        assert_eq!(scalar.expect_scalar(), &UniScalar::I32);

        let array = UniDataType::from_array(Box::new(UniDataType::Scalar(UniScalar::String)));
        assert!(matches!(
            array.as_array().map(Box::as_ref),
            Some(UniDataType::Scalar(UniScalar::String))
        ));
        assert!(array.as_scalar().is_none());
        assert!(matches!(
            array.expect_array().as_ref(),
            UniDataType::Scalar(UniScalar::String)
        ));

        let record = UniDataType::from_record(sample_record_type());
        assert!(record.as_record().is_some());
        assert!(record.as_scalar().is_none());
        let inner = record.expect_record();
        assert_eq!(inner.record_name, "vote_record");

        let option = UniDataType::from_option(Box::new(UniDataType::Scalar(UniScalar::I64)));
        assert!(matches!(
            option.as_option().map(Box::as_ref),
            Some(UniDataType::Scalar(UniScalar::I64))
        ));
        assert!(option.as_scalar().is_none());
        assert!(matches!(
            option.expect_option().as_ref(),
            UniDataType::Scalar(UniScalar::I64)
        ));

        let tuple = UniDataType::from_tuple(vec![
            UniDataType::Scalar(UniScalar::I32),
            UniDataType::Scalar(UniScalar::String),
        ]);
        let tuple_inner = tuple.as_tuple().expect("tuple");
        assert_eq!(tuple_inner.len(), 2);
        assert!(matches!(
            tuple_inner[0],
            UniDataType::Scalar(UniScalar::I32)
        ));
        assert!(matches!(
            tuple_inner[1],
            UniDataType::Scalar(UniScalar::String)
        ));
        assert!(tuple.as_scalar().is_none());
        let expect_inner = tuple.expect_tuple();
        assert_eq!(expect_inner.len(), 2);
        assert!(matches!(
            expect_inner[0],
            UniDataType::Scalar(UniScalar::I32)
        ));
        assert!(matches!(
            expect_inner[1],
            UniDataType::Scalar(UniScalar::String)
        ));

        let result = UniDataType::from_result(UniResultType {
            ok: Some(Box::new(UniDataType::Scalar(UniScalar::I32))),
            err: Some(Box::new(UniDataType::Scalar(UniScalar::String))),
        });
        assert!(result.as_result().is_some());
        assert!(result.as_scalar().is_none());
        let inner = result.expect_result();
        assert!(inner.ok.is_some());
        assert!(inner.err.is_some());

        let identifier = UniDataType::from_identifier("MyType".to_string());
        assert_eq!(identifier.as_identifier(), Some(&"MyType".to_string()));
        assert!(identifier.as_scalar().is_none());
        assert_eq!(identifier.expect_identifier(), "MyType");

        let boxed = UniDataType::from_box(Box::new(UniDataType::Scalar(UniScalar::I32)));
        assert!(matches!(
            boxed.as_box().map(Box::as_ref),
            Some(UniDataType::Scalar(UniScalar::I32))
        ));
        assert!(matches!(
            boxed.expect_box().as_ref(),
            UniDataType::Scalar(UniScalar::I32)
        ));

        let binary = UniDataType::Binary;
        assert!(binary.as_scalar().is_none());
    }

    #[test]
    fn serde_roundtrip_for_variants() {
        assert_json_and_binary_roundtrip(&UniDataType::Scalar(UniScalar::I32));
        assert_json_and_binary_roundtrip(&UniDataType::Array(Box::new(UniDataType::Scalar(
            UniScalar::String,
        ))));
        assert_json_and_binary_roundtrip(&UniDataType::Record(sample_record_type()));
        assert_json_and_binary_roundtrip(&UniDataType::Option(Box::new(UniDataType::Scalar(
            UniScalar::I64,
        ))));
        assert_json_and_binary_roundtrip(&UniDataType::Tuple(vec![
            UniDataType::Scalar(UniScalar::I32),
            UniDataType::Scalar(UniScalar::String),
        ]));
        assert_json_and_binary_roundtrip(&UniDataType::Result(UniResultType {
            ok: Some(Box::new(UniDataType::Scalar(UniScalar::I32))),
            err: Some(Box::new(UniDataType::Identifier("err".to_string()))),
        }));
        assert_json_and_binary_roundtrip(&UniDataType::Identifier("MyType".to_string()));
        assert_json_and_binary_roundtrip(&UniDataType::Binary);
        assert_json_and_binary_roundtrip(&UniDataType::Box(Box::new(UniDataType::Scalar(
            UniScalar::I32,
        ))));
    }

    #[test]
    fn deserialize_rejects_invalid_and_truncated_tags() {
        assert!(deserialize_from_json::<UniDataType>("[99,0]").is_err());
        assert!(deserialize_from_json::<UniDataType>("[0]").is_err());
        assert!(deserialize_from_json::<UniDataType>("[7]").is_err());
    }

    #[test]
    fn json_shape_sanity() {
        let scalar_json = serialize_to_json(&UniDataType::Scalar(UniScalar::I32)).unwrap();
        let scalar_compact: String = scalar_json.chars().filter(|c| !c.is_whitespace()).collect();
        assert_eq!(scalar_compact, "[0,6]");

        let binary_json = serialize_to_json(&UniDataType::Binary).unwrap();
        let binary_compact: String = binary_json.chars().filter(|c| !c.is_whitespace()).collect();
        assert_eq!(binary_compact, "[7,0]");

        let decoded: UniDataType = deserialize_from_json("[7,0]").unwrap();
        assert!(matches!(decoded, UniDataType::Binary));
    }

    #[test]
    fn nested_record_roundtrip() {
        assert_json_and_binary_roundtrip(&sample_data_type());
    }
}
