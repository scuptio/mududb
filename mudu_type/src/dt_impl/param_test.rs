use crate::dat_type::DatType;
use crate::dat_type_id::DatTypeID;
use crate::dt_impl::dt_create::{create_array_type, create_object_type, create_string_type};
use crate::dtp_numeric::{DTPNumeric, NUMERIC_MAX_PRECISION, NUMERIC_MAX_SCALE};
use mudu::common::default_value::DT_CHAR_FIXED_LEN_DEFAULT;

fn assert_param_input_roundtrip(id: DatTypeID, dt: DatType) {
    let info = dt.to_info();
    let input = id.opt_fn_param().as_ref().unwrap().input;
    let parsed = input(&info.param).unwrap();

    assert_eq!(parsed.dat_type_id(), id);
    assert_eq!(parsed.to_info().id, info.id);
    assert_eq!(parsed.to_info().param, info.param);

    let reparsed = DatType::from_info(&parsed.to_info()).unwrap();
    assert_eq!(reparsed.to_info().id, info.id);
    assert_eq!(reparsed.to_info().param, info.param);
}

#[test]
fn string_param_input_parses_and_roundtrips() {
    assert_param_input_roundtrip(DatTypeID::String, create_string_type(Some(48)));
}

#[test]
fn string_param_default_matches_registered_default() {
    let default = DatTypeID::String.fn_param_default().unwrap()();
    assert_eq!(default.dat_type_id(), DatTypeID::String);

    let string_param = default.expect_string_param();
    assert_eq!(string_param.length(), DT_CHAR_FIXED_LEN_DEFAULT as u32);
}

#[test]
fn numeric_param_input_parses_and_roundtrips() {
    assert_param_input_roundtrip(
        DatTypeID::Numeric,
        DatType::from_numeric(DTPNumeric::new(18, 4)),
    );
}

#[test]
fn numeric_param_default_matches_registered_default() {
    let default = DatTypeID::Numeric.fn_param_default().unwrap()();
    assert_eq!(default.dat_type_id(), DatTypeID::Numeric);

    let numeric_param = default.expect_numeric_param();
    assert_eq!(numeric_param.precision(), NUMERIC_MAX_PRECISION);
    assert_eq!(numeric_param.scale(), 0);
}

#[test]
fn array_param_input_parses_nested_type() {
    let dt = create_array_type(create_string_type(Some(16)));
    assert_param_input_roundtrip(DatTypeID::Array, dt);
}

#[test]
fn object_param_input_parses_record_schema() {
    let dt = create_object_type(
        "user_profile".to_string(),
        vec![
            ("name".to_string(), create_string_type(Some(32))),
            (
                "tags".to_string(),
                create_array_type(DatType::new_no_param(DatTypeID::Binary)),
            ),
            ("age".to_string(), DatType::new_no_param(DatTypeID::I32)),
        ],
    );
    assert_param_input_roundtrip(DatTypeID::Record, dt);
}

#[test]
fn param_input_rejects_invalid_json() {
    let string_err = (DatTypeID::String.opt_fn_param().as_ref().unwrap().input)("{");
    assert!(string_err.is_err());

    let numeric_err = (DatTypeID::Numeric.opt_fn_param().as_ref().unwrap().input)("{");
    assert!(numeric_err.is_err());

    let array_err = (DatTypeID::Array.opt_fn_param().as_ref().unwrap().input)("{");
    assert!(array_err.is_err());

    let record_err = (DatTypeID::Record.opt_fn_param().as_ref().unwrap().input)("{");
    assert!(record_err.is_err());
}

#[test]
fn numeric_param_validation_rejects_out_of_range_values() {
    assert!(DTPNumeric::new(0, 0).validate().is_err());
    assert!(
        DTPNumeric::new(NUMERIC_MAX_PRECISION + 1, 0)
            .validate()
            .is_err()
    );
    assert!(
        DTPNumeric::new(NUMERIC_MAX_PRECISION, NUMERIC_MAX_SCALE + 1)
            .validate()
            .is_err()
    );
    assert!(DTPNumeric::new(4, 5).validate().is_err());
}
