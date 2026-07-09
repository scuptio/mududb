use crate::data_type::DataType;
use crate::data_type_impl::data_type_create::{
    create_array_type, create_object_type, create_string_type,
};
use crate::data_type_param_numeric::DataTypeParamNumeric;
use crate::data_value::DataValue;
use crate::type_error::{TyEC, TyErr};
use crate::type_family::TypeFamily;
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
            TypeFamily::I32,
            DataType::new_no_param(TypeFamily::I32),
            "\"bad\"",
        ),
        (
            TypeFamily::I64,
            DataType::new_no_param(TypeFamily::I64),
            "\"bad\"",
        ),
        (
            TypeFamily::F32,
            DataType::new_no_param(TypeFamily::F32),
            "\"bad\"",
        ),
        (
            TypeFamily::F64,
            DataType::new_no_param(TypeFamily::F64),
            "\"bad\"",
        ),
        (TypeFamily::String, create_string_type(Some(8)), "not-json"),
        (
            TypeFamily::U128,
            DataType::new_no_param(TypeFamily::U128),
            "\"not-a-u128\"",
        ),
        (
            TypeFamily::I128,
            DataType::new_no_param(TypeFamily::I128),
            "\"not-an-i128\"",
        ),
        (
            TypeFamily::Binary,
            DataType::new_no_param(TypeFamily::Binary),
            "{\"oops\":1}",
        ),
        (
            TypeFamily::Numeric,
            DataType::from_numeric(DataTypeParamNumeric::new(9, 2)),
            "\"bad\"",
        ),
        (
            TypeFamily::Array,
            create_array_type(DataType::new_no_param(TypeFamily::I32)),
            "{\"oops\":1}",
        ),
        (
            TypeFamily::Record,
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
            TypeFamily::I32,
            DataType::new_no_param(TypeFamily::I32),
            "{\"abc\"",
        ),
        (
            TypeFamily::I64,
            DataType::new_no_param(TypeFamily::I64),
            "{\"abc\"",
        ),
        (
            TypeFamily::F32,
            DataType::new_no_param(TypeFamily::F32),
            "{\"abc\"",
        ),
        (
            TypeFamily::F64,
            DataType::new_no_param(TypeFamily::F64),
            "{\"abc\"",
        ),
        (TypeFamily::String, create_string_type(Some(8)), "{ 123"),
        (
            TypeFamily::U128,
            DataType::new_no_param(TypeFamily::U128),
            "{true",
        ),
        (
            TypeFamily::I128,
            DataType::new_no_param(TypeFamily::I128),
            "{ false",
        ),
        (
            TypeFamily::Binary,
            DataType::new_no_param(TypeFamily::Binary),
            "{ [\"bad\"]",
        ),
        (
            TypeFamily::Numeric,
            DataType::from_numeric(DataTypeParamNumeric::new(9, 2)),
            "{ [\"bad\"]",
        ),
        (
            TypeFamily::Array,
            create_array_type(DataType::new_no_param(TypeFamily::I32)),
            "{[\"bad\"]",
        ),
        (
            TypeFamily::Record,
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

    let err = TypeFamily::String.fn_input()("not-json", &dt).unwrap_err();
    assert_ty_ec(err, TyEC::TypeConvertFailed);

    let err = TypeFamily::String.fn_input_json()(&JsonValue::Bool(true), &dt).unwrap_err();
    assert_ty_ec(err, TyEC::TypeConvertFailed);

    let err = TypeFamily::String.fn_recv()(&[0, 0], &dt).unwrap_err();
    assert_ty_ec(err, TyEC::TypeConvertFailed);

    let value = DataValue::from_string("abcdef".to_string());
    let err = TypeFamily::String.fn_send_to()(&value, &dt, &mut [0u8; 4]).unwrap_err();
    assert_ty_ec(err, TyEC::TypeConvertFailed);
}

#[test]
fn binary_error_paths_return_expected_error_codes() {
    let dt = DataType::new_no_param(TypeFamily::Binary);

    let err = TypeFamily::Binary.fn_input_json()(&JsonValue::String("oops".to_string()), &dt)
        .unwrap_err();
    assert_ty_ec(err, TyEC::TypeConvertFailed);

    let err = TypeFamily::Binary.fn_input_json()(
        &JsonValue::Array(vec![JsonValue::String("bad".to_string())]),
        &dt,
    )
    .unwrap_err();
    assert_ty_ec(err, TyEC::TypeConvertFailed);

    let err = TypeFamily::Binary.fn_recv()(&[0, 0, 0], &dt).unwrap_err();
    assert_ty_ec(err, TyEC::InsufficientSpace);

    let value = DataValue::from_binary(vec![1, 2, 3]);
    let err = TypeFamily::Binary.fn_send_to()(&value, &dt, &mut [0u8; 2]).unwrap_err();
    assert_ty_ec(err, TyEC::InsufficientSpace);
}

#[test]
fn numeric_error_paths_return_expected_error_codes() {
    let dt = DataType::from_numeric(DataTypeParamNumeric::new(9, 2));

    let err = TypeFamily::Numeric.fn_input_json()(&JsonValue::Bool(true), &dt).unwrap_err();
    assert_ty_ec(err, TyEC::TypeConvertFailed);

    let err = TypeFamily::Numeric.fn_recv()(&[0u8; 8], &dt).unwrap_err();
    assert_ty_ec(err, TyEC::InsufficientSpace);

    let value = DataValue::from_numeric(mudu::data_type::numeric::Numeric::parse("7.50").unwrap());
    let err = TypeFamily::Numeric.fn_send_to()(&value, &dt, &mut [0u8; 8]).unwrap_err();
    assert_ty_ec(err, TyEC::InsufficientSpace);

    let overflow =
        DataValue::from_numeric(mudu::data_type::numeric::Numeric::parse("123456789.00").unwrap());
    let err = match TypeFamily::Numeric.fn_send()(&overflow, &dt) {
        Ok(_) => panic!("expected numeric send precision overflow"),
        Err(err) => err,
    };
    assert_ty_ec(err, TyEC::TypeConvertFailed);
}

#[test]
fn array_error_paths_return_expected_error_codes() {
    let dt = create_array_type(DataType::new_no_param(TypeFamily::I32));

    let err =
        TypeFamily::Array.fn_input_json()(&JsonValue::String("oops".to_string()), &dt).unwrap_err();
    assert_ty_ec(err, TyEC::TypeConvertFailed);

    let err = TypeFamily::Array.fn_input_json()(
        &JsonValue::Array(vec![JsonValue::String("bad".to_string())]),
        &dt,
    )
    .unwrap_err();
    assert_ty_ec(err, TyEC::TypeConvertFailed);

    let err = TypeFamily::Array.fn_recv()(&[0, 0, 0, 0], &dt).unwrap_err();
    assert_ty_ec(err, TyEC::InsufficientSpace);
}

#[test]
fn object_error_paths_return_expected_error_codes() {
    let dt = create_object_type(
        "user".to_string(),
        vec![
            ("name".to_string(), create_string_type(Some(16))),
            ("age".to_string(), DataType::new_no_param(TypeFamily::I32)),
        ],
    );

    let err = TypeFamily::Record.fn_input_json()(
        &JsonValue::Object(
            [("name".to_string(), JsonValue::String("neo".to_string()))]
                .into_iter()
                .collect(),
        ),
        &dt,
    )
    .unwrap_err();
    assert_ty_ec(err, TyEC::TypeConvertFailed);

    let err = TypeFamily::Record.fn_output_json()(
        &DataValue::from_record(vec![DataValue::from_string("neo".to_string())]),
        &dt,
    )
    .err()
    .unwrap();
    assert_ty_ec(err, TyEC::TypeConvertFailed);

    let value = DataValue::from_record(vec![
        DataValue::from_string("neo".to_string()),
        DataValue::from_i32(7),
    ]);
    let err = TypeFamily::Record.fn_send_to()(&value, &dt, &mut [0u8; 4]).unwrap_err();
    assert_ty_ec(err, TyEC::InsufficientSpace);

    let err = TypeFamily::Record.fn_recv()(&[0, 0, 0, 0], &dt).unwrap_err();
    assert_ty_ec(err, TyEC::InsufficientSpace);
}
