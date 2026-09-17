use super::RustType;
use mudu::error::ErrorCode;
use mudu_binding::universal::uni_data_type::UniDataType;
use mudu_binding::universal::uni_scalar::UniScalar;
use mudu_binding::universal::uni_type_desc::UniTypeDesc;
use mudu_type::type_family::TypeFamily;
use std::error::Error;

fn custom_types() -> UniTypeDesc {
    let mut desc = UniTypeDesc::default();
    desc.types
        .insert("MyI32".to_string(), UniDataType::Scalar(UniScalar::I32));
    desc
}

#[test]
fn is_vec_u8_recognizes_byte_vector() {
    assert!(
        RustType::Generic(
            "Vec".to_string(),
            vec![RustType::Primitive("u8".to_string())]
        )
        .is_vec_u8()
    );
    assert!(
        !RustType::Generic(
            "Vec".to_string(),
            vec![RustType::Primitive("i32".to_string())]
        )
        .is_vec_u8()
    );
    assert!(!RustType::Primitive("u8".to_string()).is_vec_u8());
}

#[test]
fn as_ret_type_extracts_inner_types() -> Result<(), Box<dyn Error>> {
    let ok = RustType::Generic(
        "RS".to_string(),
        vec![RustType::Tuple(vec![
            RustType::Primitive("i32".to_string()),
            RustType::Primitive("String".to_string()),
        ])],
    );
    let inner = ok.as_ret_type()?;
    assert_eq!(inner.len(), 2);

    let wrong_inner_count = RustType::Generic(
        "RS".to_string(),
        vec![
            RustType::Primitive("i32".to_string()),
            RustType::Primitive("i64".to_string()),
        ],
    );
    let err = wrong_inner_count
        .as_ret_type()
        .err()
        .ok_or("expected an error")?;
    assert_eq!(err.ec(), ErrorCode::InvalidType);

    let not_generic = RustType::Primitive("i32".to_string());
    let err = not_generic.as_ret_type().err().ok_or("expected an error")?;
    assert_eq!(err.ec(), ErrorCode::InvalidType);
    Ok(())
}

#[test]
fn to_type_str_renders_variants() {
    assert_eq!(RustType::Primitive("i32".to_string()).to_type_str(), "i32");
    assert_eq!(RustType::Custom("OID".to_string()).to_type_str(), "OID");
    let tuple = RustType::Tuple(vec![
        RustType::Primitive("i32".to_string()),
        RustType::Primitive("String".to_string()),
    ]);
    assert_eq!(tuple.to_type_str(), "(i32, String, )");
    let generic = RustType::Generic(
        "Vec".to_string(),
        vec![RustType::Primitive("u8".to_string())],
    );
    assert_eq!(generic.to_type_str(), "Vec<u8, >");
}

#[test]
fn to_ret_type_str_extracts_strings() -> Result<(), Box<dyn Error>> {
    let ok = RustType::Generic(
        "RS".to_string(),
        vec![RustType::Tuple(vec![
            RustType::Primitive("i32".to_string()),
            RustType::Custom("OID".to_string()),
        ])],
    );
    assert_eq!(ok.to_ret_type_str()?, vec!["i32", "OID"]);

    let single = RustType::Generic(
        "RS".to_string(),
        vec![RustType::Primitive("i64".to_string())],
    );
    assert_eq!(single.to_ret_type_str()?, vec!["i64"]);

    let invalid = RustType::Generic(
        "RS".to_string(),
        vec![
            RustType::Primitive("i32".to_string()),
            RustType::Primitive("i64".to_string()),
        ],
    );
    assert_eq!(
        invalid
            .to_ret_type_str()
            .err()
            .ok_or("expected an error")?
            .ec(),
        ErrorCode::InvalidType
    );
    Ok(())
}

#[test]
fn to_data_type_maps_primitives_and_customs() -> Result<(), Box<dyn Error>> {
    let custom = custom_types();

    assert_eq!(
        RustType::Primitive("i128".to_string())
            .to_data_type(&custom)?
            .type_family(),
        TypeFamily::I128
    );
    assert_eq!(
        RustType::Primitive("u128".to_string())
            .to_data_type(&custom)?
            .type_family(),
        TypeFamily::U128
    );
    assert_eq!(
        RustType::Primitive("f32".to_string())
            .to_data_type(&custom)?
            .type_family(),
        TypeFamily::F32
    );
    assert_eq!(
        RustType::Primitive("f64".to_string())
            .to_data_type(&custom)?
            .type_family(),
        TypeFamily::F64
    );
    assert_eq!(
        RustType::Custom("OID".to_string())
            .to_data_type(&custom)?
            .type_family(),
        TypeFamily::U128
    );
    assert_eq!(
        RustType::Custom("String".to_string())
            .to_data_type(&custom)?
            .type_family(),
        TypeFamily::String
    );
    assert_eq!(
        RustType::Custom("MyI32".to_string())
            .to_data_type(&custom)?
            .type_family(),
        TypeFamily::I32
    );
    Ok(())
}

#[test]
fn to_data_type_rejects_unknown_types() -> Result<(), Box<dyn Error>> {
    let custom = custom_types();

    let err = RustType::Primitive("bool".to_string())
        .to_data_type(&custom)
        .err()
        .ok_or("expected an error")?;
    assert_eq!(err.ec(), ErrorCode::InvalidType);

    let err = RustType::Custom("Unknown".to_string())
        .to_data_type(&custom)
        .err()
        .ok_or("expected an error")?;
    assert_eq!(err.ec(), ErrorCode::EntityNotFound);
    Ok(())
}

#[test]
fn to_data_type_handles_vec_and_vec_u8() -> Result<(), Box<dyn Error>> {
    let custom = custom_types();

    let vec_u8 = RustType::Generic(
        "Vec".to_string(),
        vec![RustType::Primitive("u8".to_string())],
    );
    assert_eq!(
        vec_u8.to_data_type(&custom)?.type_family(),
        TypeFamily::Binary
    );

    let vec_i32 = RustType::Generic(
        "Vec".to_string(),
        vec![RustType::Primitive("i32".to_string())],
    );
    let dt = vec_i32.to_data_type(&custom)?;
    assert_eq!(dt.type_family(), TypeFamily::Array);

    let unsupported = RustType::Generic(
        "Option".to_string(),
        vec![RustType::Primitive("i32".to_string())],
    );
    assert_eq!(
        unsupported
            .to_data_type(&custom)
            .err()
            .ok_or("expected an error")?
            .ec(),
        ErrorCode::InvalidType
    );
    Ok(())
}

#[test]
fn tuple_and_custom_to_data_type_are_rejected() -> Result<(), Box<dyn Error>> {
    let custom = custom_types();
    let tuple = RustType::Tuple(vec![RustType::Primitive("i32".to_string())]);
    assert_eq!(
        tuple
            .to_data_type(&custom)
            .err()
            .ok_or("expected an error")?
            .ec(),
        ErrorCode::InvalidType
    );
    Ok(())
}
