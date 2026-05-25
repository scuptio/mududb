use crate::dat_type::DatType;
use crate::dat_type_id::DatTypeID;
use crate::dat_value::DatValue;
use crate::dt_impl::dt_create::{create_array_type, create_object_type, create_string_type};
use crate::dtp_numeric::DTPNumeric;
use crate::type_error::{TyEC, TyErr};
use mudu::utils::json::JsonValue;

fn assert_ty_ec(err: TyErr, ec: TyEC) {
    assert_eq!(
        std::mem::discriminant(&err.ec()),
        std::mem::discriminant(&ec)
    );
}

#[test]
fn invalid_textual_input_paths_return_type_convert_failed() {
    let cases = vec![
        (
            DatTypeID::I32,
            DatType::new_no_param(DatTypeID::I32),
            "\"bad\"",
        ),
        (
            DatTypeID::I64,
            DatType::new_no_param(DatTypeID::I64),
            "\"bad\"",
        ),
        (
            DatTypeID::F32,
            DatType::new_no_param(DatTypeID::F32),
            "\"bad\"",
        ),
        (
            DatTypeID::F64,
            DatType::new_no_param(DatTypeID::F64),
            "\"bad\"",
        ),
        (DatTypeID::String, create_string_type(Some(8)), "not-json"),
        (
            DatTypeID::U128,
            DatType::new_no_param(DatTypeID::U128),
            "\"not-a-u128\"",
        ),
        (
            DatTypeID::I128,
            DatType::new_no_param(DatTypeID::I128),
            "\"not-an-i128\"",
        ),
        (
            DatTypeID::Binary,
            DatType::new_no_param(DatTypeID::Binary),
            "{\"oops\":1}",
        ),
        (
            DatTypeID::Numeric,
            DatType::from_numeric(DTPNumeric::new(9, 2)),
            "\"bad\"",
        ),
        (
            DatTypeID::Array,
            create_array_type(DatType::new_no_param(DatTypeID::I32)),
            "{\"oops\":1}",
        ),
        (
            DatTypeID::Record,
            create_object_type(
                "user".to_string(),
                vec![("name".to_string(), create_string_type(Some(16)))],
            ),
            "[1,2,3]",
        ),
    ];

    for (id, dt, textual) in cases {
        let err = id.fn_input()(textual, &dt).unwrap_err();
        assert_ty_ec(err, TyEC::TypeConvertFailed);
    }
}

#[test]
fn textual_input_rejects_json_with_wrong_shape() {
    let cases = vec![
        (
            DatTypeID::I32,
            DatType::new_no_param(DatTypeID::I32),
            "{\"abc\"",
        ),
        (
            DatTypeID::I64,
            DatType::new_no_param(DatTypeID::I64),
            "{\"abc\"",
        ),
        (
            DatTypeID::F32,
            DatType::new_no_param(DatTypeID::F32),
            "{\"abc\"",
        ),
        (
            DatTypeID::F64,
            DatType::new_no_param(DatTypeID::F64),
            "{\"abc\"",
        ),
        (DatTypeID::String, create_string_type(Some(8)), "{ 123"),
        (
            DatTypeID::U128,
            DatType::new_no_param(DatTypeID::U128),
            "{true",
        ),
        (
            DatTypeID::I128,
            DatType::new_no_param(DatTypeID::I128),
            "{ false",
        ),
        (
            DatTypeID::Binary,
            DatType::new_no_param(DatTypeID::Binary),
            "{ [\"bad\"]",
        ),
        (
            DatTypeID::Numeric,
            DatType::from_numeric(DTPNumeric::new(9, 2)),
            "{ [\"bad\"]",
        ),
        (
            DatTypeID::Array,
            create_array_type(DatType::new_no_param(DatTypeID::I32)),
            "{[\"bad\"]",
        ),
        (
            DatTypeID::Record,
            create_object_type(
                "user".to_string(),
                vec![("name".to_string(), create_string_type(Some(16)))],
            ),
            "{\"name\":123",
        ),
    ];

    for (id, dt, textual) in cases {
        let err = id.fn_input()(textual, &dt).unwrap_err();
        assert_ty_ec(err, TyEC::TypeConvertFailed);
    }
}

#[test]
fn string_error_paths_return_expected_error_codes() {
    let dt = create_string_type(Some(8));

    let err = DatTypeID::String.fn_input()("not-json", &dt).unwrap_err();
    assert_ty_ec(err, TyEC::TypeConvertFailed);

    let err = DatTypeID::String.fn_input_json()(&JsonValue::Bool(true), &dt).unwrap_err();
    assert_ty_ec(err, TyEC::TypeConvertFailed);

    let err = DatTypeID::String.fn_recv()(&[0, 0], &dt).unwrap_err();
    assert_ty_ec(err, TyEC::TypeConvertFailed);

    let value = DatValue::from_string("abcdef".to_string());
    let err = DatTypeID::String.fn_send_to()(&value, &dt, &mut [0u8; 4]).unwrap_err();
    assert_ty_ec(err, TyEC::TypeConvertFailed);
}

#[test]
fn binary_error_paths_return_expected_error_codes() {
    let dt = DatType::new_no_param(DatTypeID::Binary);

    let err =
        DatTypeID::Binary.fn_input_json()(&JsonValue::String("oops".to_string()), &dt).unwrap_err();
    assert_ty_ec(err, TyEC::TypeConvertFailed);

    let err = DatTypeID::Binary.fn_input_json()(
        &JsonValue::Array(vec![JsonValue::String("bad".to_string())]),
        &dt,
    )
    .unwrap_err();
    assert_ty_ec(err, TyEC::TypeConvertFailed);

    let err = DatTypeID::Binary.fn_recv()(&[0, 0, 0], &dt).unwrap_err();
    assert_ty_ec(err, TyEC::InsufficientSpace);

    let value = DatValue::from_binary(vec![1, 2, 3]);
    let err = DatTypeID::Binary.fn_send_to()(&value, &dt, &mut [0u8; 2]).unwrap_err();
    assert_ty_ec(err, TyEC::InsufficientSpace);
}

#[test]
fn numeric_error_paths_return_expected_error_codes() {
    let dt = DatType::from_numeric(DTPNumeric::new(9, 2));

    let err = DatTypeID::Numeric.fn_input_json()(&JsonValue::Bool(true), &dt).unwrap_err();
    assert_ty_ec(err, TyEC::TypeConvertFailed);

    let err = DatTypeID::Numeric.fn_recv()(&[0u8; 8], &dt).unwrap_err();
    assert_ty_ec(err, TyEC::InsufficientSpace);

    let value = DatValue::from_numeric(mudu::data_type::numeric::Numeric::parse("7.50").unwrap());
    let err = DatTypeID::Numeric.fn_send_to()(&value, &dt, &mut [0u8; 8]).unwrap_err();
    assert_ty_ec(err, TyEC::InsufficientSpace);

    let overflow =
        DatValue::from_numeric(mudu::data_type::numeric::Numeric::parse("123456789.00").unwrap());
    let err = match DatTypeID::Numeric.fn_send()(&overflow, &dt) {
        Ok(_) => panic!("expected numeric send precision overflow"),
        Err(err) => err,
    };
    assert_ty_ec(err, TyEC::TypeConvertFailed);
}

#[test]
fn array_error_paths_return_expected_error_codes() {
    let dt = create_array_type(DatType::new_no_param(DatTypeID::I32));

    let err =
        DatTypeID::Array.fn_input_json()(&JsonValue::String("oops".to_string()), &dt).unwrap_err();
    assert_ty_ec(err, TyEC::TypeConvertFailed);

    let err = DatTypeID::Array.fn_input_json()(
        &JsonValue::Array(vec![JsonValue::String("bad".to_string())]),
        &dt,
    )
    .unwrap_err();
    assert_ty_ec(err, TyEC::TypeConvertFailed);

    let err = DatTypeID::Array.fn_recv()(&[0, 0, 0, 0], &dt).unwrap_err();
    assert_ty_ec(err, TyEC::InsufficientSpace);
}

#[test]
fn object_error_paths_return_expected_error_codes() {
    let dt = create_object_type(
        "user".to_string(),
        vec![
            ("name".to_string(), create_string_type(Some(16))),
            ("age".to_string(), DatType::new_no_param(DatTypeID::I32)),
        ],
    );

    let err = DatTypeID::Record.fn_input_json()(
        &JsonValue::Object(
            [("name".to_string(), JsonValue::String("neo".to_string()))]
                .into_iter()
                .collect(),
        ),
        &dt,
    )
    .unwrap_err();
    assert_ty_ec(err, TyEC::TypeConvertFailed);

    let err = DatTypeID::Record.fn_output_json()(
        &DatValue::from_record(vec![DatValue::from_string("neo".to_string())]),
        &dt,
    )
    .err()
    .unwrap();
    assert_ty_ec(err, TyEC::TypeConvertFailed);

    let value = DatValue::from_record(vec![
        DatValue::from_string("neo".to_string()),
        DatValue::from_i32(7),
    ]);
    let err = DatTypeID::Record.fn_send_to()(&value, &dt, &mut [0u8; 4]).unwrap_err();
    assert_ty_ec(err, TyEC::InsufficientSpace);

    let err = DatTypeID::Record.fn_recv()(&[0, 0, 0, 0], &dt).unwrap_err();
    assert_ty_ec(err, TyEC::InsufficientSpace);
}
