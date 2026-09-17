use crate::data_type::DataType;
use crate::data_type_impl::data_type_create::{
    create_array_type, create_object_type, create_string_type,
};
use crate::data_type_param_numeric::{
    DataTypeParamNumeric, NUMERIC_MAX_PRECISION, NUMERIC_MAX_SCALE,
};
use crate::type_family::TypeFamily;
use mudu::common::default_value::DT_CHAR_FIXED_LEN_DEFAULT;

fn assert_param_input_roundtrip(id: TypeFamily, dt: DataType) {
    let info = dt.to_info();
    let input = id.opt_fn_param().as_ref().unwrap().input;
    let parsed = input(&info.param).unwrap();

    assert_eq!(parsed.type_family(), id);
    assert_eq!(parsed.to_info().id, info.id);
    assert_eq!(parsed.to_info().param, info.param);

    let reparsed = DataType::from_info(&parsed.to_info()).unwrap();
    assert_eq!(reparsed.to_info().id, info.id);
    assert_eq!(reparsed.to_info().param, info.param);
}

#[test]
fn string_param_input_parses_and_roundtrips() {
    assert_param_input_roundtrip(TypeFamily::String, create_string_type(Some(48)));
}

#[test]
fn string_param_default_matches_registered_default() {
    let default = TypeFamily::String.fn_param_default().unwrap()();
    assert_eq!(default.type_family(), TypeFamily::String);

    let string_param = default.expect_string_param();
    assert_eq!(string_param.length(), DT_CHAR_FIXED_LEN_DEFAULT as u32);
}

#[test]
fn numeric_param_input_parses_and_roundtrips() {
    assert_param_input_roundtrip(
        TypeFamily::Numeric,
        DataType::from_numeric(DataTypeParamNumeric::new(18, 4)),
    );
}

#[test]
fn numeric_param_default_matches_registered_default() {
    let default = TypeFamily::Numeric.fn_param_default().unwrap()();
    assert_eq!(default.type_family(), TypeFamily::Numeric);

    let numeric_param = default.expect_numeric_param();
    assert_eq!(numeric_param.precision(), NUMERIC_MAX_PRECISION);
    assert_eq!(numeric_param.scale(), 0);
}

#[test]
fn array_param_input_parses_nested_type() {
    let dt = create_array_type(create_string_type(Some(16)));
    assert_param_input_roundtrip(TypeFamily::Array, dt);
}

#[test]
fn object_param_input_parses_record_schema() {
    let dt = create_object_type(
        "user_profile".to_string(),
        vec![
            ("name".to_string(), create_string_type(Some(32))),
            (
                "tags".to_string(),
                create_array_type(DataType::new_no_param(TypeFamily::Binary)),
            ),
            ("age".to_string(), DataType::new_no_param(TypeFamily::I32)),
        ],
    );
    assert_param_input_roundtrip(TypeFamily::Record, dt);
}

#[test]
fn param_input_rejects_invalid_json() {
    let string_err = (TypeFamily::String.opt_fn_param().as_ref().unwrap().input)("{");
    assert!(string_err.is_err());

    let numeric_err = (TypeFamily::Numeric.opt_fn_param().as_ref().unwrap().input)("{");
    assert!(numeric_err.is_err());

    let array_err = (TypeFamily::Array.opt_fn_param().as_ref().unwrap().input)("{");
    assert!(array_err.is_err());

    let record_err = (TypeFamily::Record.opt_fn_param().as_ref().unwrap().input)("{");
    assert!(record_err.is_err());
}

#[test]
fn numeric_param_validation_rejects_out_of_range_values() {
    assert!(DataTypeParamNumeric::new(0, 0).validate().is_err());
    assert!(
        DataTypeParamNumeric::new(NUMERIC_MAX_PRECISION + 1, 0)
            .validate()
            .is_err()
    );
    assert!(
        DataTypeParamNumeric::new(NUMERIC_MAX_PRECISION, NUMERIC_MAX_SCALE + 1)
            .validate()
            .is_err()
    );
    assert!(DataTypeParamNumeric::new(4, 5).validate().is_err());
}

#[test]
fn time_param_input_parses_and_roundtrips() {
    assert_param_input_roundtrip(TypeFamily::Time, DataType::default_for(TypeFamily::Time));
}

#[test]
fn time_param_input_rejects_invalid_json() {
    let err = (TypeFamily::Time.opt_fn_param().as_ref().unwrap().input)("not-json");
    assert!(err.is_err());
}

#[test]
fn timestamp_param_input_parses_and_roundtrips() {
    assert_param_input_roundtrip(
        TypeFamily::Timestamp,
        DataType::default_for(TypeFamily::Timestamp),
    );
}

#[test]
fn timestamp_param_input_rejects_invalid_json() {
    let err = (TypeFamily::Timestamp.opt_fn_param().as_ref().unwrap().input)("not-json");
    assert!(err.is_err());
}

#[test]
fn timestamptz_param_input_parses_and_roundtrips() {
    assert_param_input_roundtrip(
        TypeFamily::TimestampTz,
        DataType::default_for(TypeFamily::TimestampTz),
    );
}

#[test]
fn timestamptz_param_input_rejects_invalid_json() {
    let err = (TypeFamily::TimestampTz
        .opt_fn_param()
        .as_ref()
        .unwrap()
        .input)("not-json");
    assert!(err.is_err());
}
