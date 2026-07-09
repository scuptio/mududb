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
fn unsupported_data_type_to_uni_fails() {
    // A plain binary DataType has no UniDataType mapping.
    let dat = DataType::new_no_param(TypeFamily::Binary);
    let err = UniDataType::uni_from(dat).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::InvalidType);
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
