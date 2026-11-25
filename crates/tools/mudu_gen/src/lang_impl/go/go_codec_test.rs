use super::go_codec::{
    go_comments, go_exported, go_from_value, go_to_value, go_type_name, go_variant_default,
    go_wire_from_any, go_zero,
};
use mudu::common::result::RS;
use mudu_binding::universal::uni_data_type::UniDataType;
use mudu_binding::universal::uni_result_type::UniResultType;
use mudu_binding::universal::uni_scalar::UniScalar;

#[test]
fn type_names_cover_the_scalar_matrix() -> RS<()> {
    // every scalar width maps to its natural Go type
    for (scalar, want) in [
        (UniScalar::U8, "uint8"),
        (UniScalar::U16, "uint16"),
        (UniScalar::U32, "uint32"),
        (UniScalar::I8, "int8"),
        (UniScalar::I16, "int16"),
        (UniScalar::I32, "int32"),
        (UniScalar::F64, "float64"),
        (UniScalar::Char, "string"),
        (UniScalar::String, "string"),
        (UniScalar::Numeric, "string"),
        (UniScalar::Date, "string"),
        (UniScalar::Time, "string"),
        (UniScalar::Timestamp, "string"),
        (UniScalar::TimestampTz, "string"),
    ] {
        assert_eq!(go_type_name(&UniDataType::Scalar(scalar))?, want);
    }
    // 128-bit integers travel as their 16-byte big-endian form
    assert_eq!(
        go_type_name(&UniDataType::Scalar(UniScalar::U128))?,
        "[16]byte"
    );
    assert_eq!(
        go_type_name(&UniDataType::Scalar(UniScalar::I128))?,
        "[16]byte"
    );
    // tuples ride on []any and boxes unwrap
    assert_eq!(
        go_type_name(&UniDataType::Tuple(vec![UniDataType::Scalar(
            UniScalar::Bool
        )]))?,
        "[]any"
    );
    assert_eq!(
        go_type_name(&UniDataType::Box(Box::new(UniDataType::Scalar(
            UniScalar::U16
        ))))?,
        "uint16"
    );
    // records and results have no Go surface name
    assert!(go_type_name(&UniDataType::Result(UniResultType::default())).is_err());
    Ok(())
}

#[test]
fn comments_collapse_doc_prefix_and_plain_lines() {
    assert_eq!(go_comments("/// doc\n// plain\n"), "// doc\n// plain");
    assert_eq!(go_comments("free text"), "// free text");
    assert_eq!(go_comments("//"), "//");
    assert_eq!(go_comments("///"), "//");
    assert_eq!(go_comments("a\n\nb"), "// a\n//\n// b");
}

#[test]
fn exported_names_are_pascal_case() {
    assert_eq!(go_exported("worker-id"), "WorkerId");
    assert_eq!(go_exported("oid"), "Oid");
}

#[test]
fn zero_values_cover_scalar_and_composite_kinds() -> RS<()> {
    let kinds = std::collections::HashMap::new();
    assert_eq!(
        go_zero(&UniDataType::Scalar(UniScalar::Bool), &kinds)?,
        "false"
    );
    assert_eq!(
        go_zero(&UniDataType::Scalar(UniScalar::U128), &kinds)?,
        "[16]byte{}"
    );
    assert_eq!(
        go_zero(&UniDataType::Scalar(UniScalar::I128), &kinds)?,
        "[16]byte{}"
    );
    assert_eq!(go_zero(&UniDataType::Scalar(UniScalar::F32), &kinds)?, "0");
    assert_eq!(
        go_zero(&UniDataType::Scalar(UniScalar::String), &kinds)?,
        "\"\""
    );
    assert_eq!(
        go_zero(&UniDataType::Scalar(UniScalar::Blob), &kinds)?,
        "nil"
    );
    assert_eq!(go_zero(&UniDataType::Binary, &kinds)?, "nil");
    assert_eq!(
        go_zero(
            &UniDataType::Array(Box::new(UniDataType::Scalar(UniScalar::U8))),
            &kinds
        )?,
        "nil"
    );
    assert_eq!(
        go_zero(&UniDataType::Option(Box::new(UniDataType::Binary)), &kinds)?,
        "nil"
    );
    assert_eq!(
        go_zero(
            &UniDataType::Tuple(vec![
                UniDataType::Scalar(UniScalar::U8),
                UniDataType::Scalar(UniScalar::String)
            ]),
            &kinds
        )?,
        "[]any{0, \"\"}"
    );
    assert_eq!(
        go_zero(
            &UniDataType::Box(Box::new(UniDataType::Scalar(UniScalar::Bool))),
            &kinds
        )?,
        "false"
    );
    assert!(go_zero(&UniDataType::Result(UniResultType::default()), &kinds).is_err());
    Ok(())
}

#[test]
fn variant_defaults_resolve_registered_variants_through_boxes() -> RS<()> {
    let mut kinds = std::collections::HashMap::new();
    kinds.insert("UniDataType".to_string(), "variant".to_string());
    kinds.insert("UniOid".to_string(), "record".to_string());
    assert_eq!(
        go_variant_default(
            &UniDataType::Identifier("uni-data-type".to_string()),
            &kinds
        )?,
        Some("defaultUniDataType()".to_string())
    );
    assert_eq!(
        go_variant_default(
            &UniDataType::Box(Box::new(UniDataType::Identifier(
                "uni-data-type".to_string()
            ))),
            &kinds
        )?,
        Some("defaultUniDataType()".to_string())
    );
    assert_eq!(
        go_variant_default(&UniDataType::Identifier("uni-oid".to_string()), &kinds)?,
        None
    );
    assert_eq!(
        go_variant_default(&UniDataType::Scalar(UniScalar::Bool), &kinds)?,
        None
    );
    Ok(())
}

#[test]
fn scalar_to_value_wraps_f32_blobs_and_wide_ints() -> RS<()> {
    let mut tmp = 0;
    assert_eq!(
        go_to_value(
            &UniDataType::Scalar(UniScalar::F32),
            "x.Ratio",
            "_d[0]",
            &mut tmp,
            "\t",
            true
        )?,
        "\t_d[0] = codec.F32(x.Ratio)"
    );
    // 128-bit integers slice to their 16-byte form
    let mut tmp = 0;
    assert_eq!(
        go_to_value(
            &UniDataType::Scalar(UniScalar::U128),
            "x.Oid",
            "_d[1]",
            &mut tmp,
            "\t",
            true
        )?,
        "\t_d[1] = x.Oid[:]"
    );
    let mut tmp = 0;
    assert_eq!(
        go_to_value(
            &UniDataType::Scalar(UniScalar::I128),
            "x.Oid",
            "_d[1]",
            &mut tmp,
            "\t",
            false
        )?,
        "\t_d[1] = blobToArrayValue(x.Oid[:])"
    );
    // scalar blob follows the func/record bin rule
    let mut tmp = 0;
    assert_eq!(
        go_to_value(
            &UniDataType::Scalar(UniScalar::Blob),
            "x.Raw",
            "_d[2]",
            &mut tmp,
            "\t",
            true
        )?,
        "\t_d[2] = x.Raw"
    );
    let mut tmp = 0;
    assert_eq!(
        go_to_value(
            &UniDataType::Scalar(UniScalar::Blob),
            "x.Raw",
            "_d[2]",
            &mut tmp,
            "\t",
            false
        )?,
        "\t_d[2] = blobToArrayValue(x.Raw)"
    );
    Ok(())
}

#[test]
fn to_value_unwraps_boxes_and_rejects_unsupported_kinds() -> RS<()> {
    let mut tmp = 0;
    let boxed = UniDataType::Box(Box::new(UniDataType::Scalar(UniScalar::U32)));
    assert_eq!(
        go_to_value(&boxed, "x.V", "_d[0]", &mut tmp, "\t", true)?,
        "\t_d[0] = x.V"
    );
    // tuples of scalars convert each element through the checked path
    let mut tmp = 0;
    let tuple = UniDataType::Tuple(vec![UniDataType::Scalar(UniScalar::U64)]);
    let out = go_to_value(&tuple, "x.Pair", "_d[3]", &mut tmp, "\t", true)?;
    assert!(out.contains("_v0 := make([]any, 1)"), "{out}");
    assert!(out.contains("_t1, err := u64Checked(x.Pair[0])"), "{out}");
    assert!(out.contains("_d[3] = _v0"), "{out}");
    // records and results are not convertible
    assert!(
        go_to_value(
            &UniDataType::Result(UniResultType::default()),
            "x",
            "_d",
            &mut 0,
            "",
            true
        )
        .is_err()
    );
    // array/tuple element errors propagate
    assert!(
        go_to_value(
            &UniDataType::Array(Box::new(UniDataType::Result(UniResultType::default()))),
            "x",
            "_d",
            &mut 0,
            "",
            true
        )
        .is_err()
    );
    assert!(
        go_to_value(
            &UniDataType::Tuple(vec![UniDataType::Result(UniResultType::default())]),
            "x",
            "_d",
            &mut 0,
            "",
            true
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn from_value_rejects_unsupported_kinds() {
    assert!(
        go_from_value(
            &UniDataType::Result(UniResultType::default()),
            "v",
            "x",
            &mut 0,
            "",
            "return nil, err"
        )
        .is_err()
    );
}

#[test]
fn wire_from_any_scalar_matrix_checks_and_assigns() -> RS<()> {
    for (scalar, helper) in [
        (UniScalar::Bool, "boolChecked"),
        (UniScalar::U8, "u8Checked"),
        (UniScalar::U16, "u16Checked"),
        (UniScalar::U32, "u32Checked"),
        (UniScalar::U64, "u64Checked"),
        (UniScalar::U128, "u128FromValue"),
        (UniScalar::I8, "i8Checked"),
        (UniScalar::I16, "i16Checked"),
        (UniScalar::I32, "i32Checked"),
        (UniScalar::I64, "i64Checked"),
        (UniScalar::I128, "i128FromValue"),
        (UniScalar::F32, "f32FromValue"),
        (UniScalar::F64, "f64Checked"),
        (UniScalar::Char, "charChecked"),
        (UniScalar::String, "strChecked"),
        (UniScalar::Numeric, "strChecked"),
        (UniScalar::Date, "strChecked"),
        (UniScalar::Time, "strChecked"),
        (UniScalar::Timestamp, "strChecked"),
        (UniScalar::TimestampTz, "strChecked"),
    ] {
        let mut tmp = 0;
        let out = go_wire_from_any(&UniDataType::Scalar(scalar), "v", "dst", &mut tmp, "", true)?;
        assert!(
            out.contains(&format!("_t0, err := {helper}(v)")),
            "{helper}: {out}"
        );
        assert!(out.contains("if err != nil {"), "{helper}: {out}");
    }
    // f32 rewraps with the marker
    let mut tmp = 0;
    let out = go_wire_from_any(
        &UniDataType::Scalar(UniScalar::F32),
        "v",
        "dst",
        &mut tmp,
        "",
        true,
    )?;
    assert!(out.contains("dst = codec.F32(_t0)"), "{out}");
    // 128-bit: bin at func level, int array in the record context
    let mut tmp = 0;
    let out = go_wire_from_any(
        &UniDataType::Scalar(UniScalar::U128),
        "v",
        "dst",
        &mut tmp,
        "",
        true,
    )?;
    assert!(out.contains("dst = _t0[:]"), "{out}");
    let mut tmp = 0;
    let out = go_wire_from_any(
        &UniDataType::Scalar(UniScalar::I128),
        "v",
        "dst",
        &mut tmp,
        "",
        false,
    )?;
    assert!(out.contains("dst = blobToArrayValue(_t0[:])"), "{out}");
    Ok(())
}

#[test]
fn wire_from_any_blob_binary_and_error_kinds() -> RS<()> {
    // blob scalar: bytesChecked at func level, blobFromValue in records
    let mut tmp = 0;
    let out = go_wire_from_any(
        &UniDataType::Scalar(UniScalar::Blob),
        "v",
        "dst",
        &mut tmp,
        "",
        true,
    )?;
    assert!(out.contains("_t0, err := bytesChecked(v)"), "{out}");
    assert!(out.contains("dst = _t0"), "{out}");
    let mut tmp = 0;
    let out = go_wire_from_any(
        &UniDataType::Scalar(UniScalar::Blob),
        "v",
        "dst",
        &mut tmp,
        "",
        false,
    )?;
    assert!(out.contains("_t0, err := blobFromValue(v)"), "{out}");
    assert!(out.contains("dst = blobToArrayValue(_t0)"), "{out}");

    // binary: array form in the record context
    let mut tmp = 0;
    let out = go_wire_from_any(&UniDataType::Binary, "v", "dst", &mut tmp, "", false)?;
    assert!(out.contains("blobFromValue(v)"), "{out}");
    assert!(out.contains("dst = blobToArrayValue(_t0)"), "{out}");

    // unsupported kinds error out
    assert!(
        go_wire_from_any(
            &UniDataType::Result(UniResultType::default()),
            "v",
            "dst",
            &mut 0,
            "",
            true
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn wire_from_any_typed_option_tuple_and_box() -> RS<()> {
    // named types assert the Go surface type, then convert
    let mut tmp = 0;
    let out = go_wire_from_any(
        &UniDataType::Identifier("uni-oid".to_string()),
        "v",
        "dst",
        &mut tmp,
        "",
        true,
    )?;
    assert!(out.contains("_t0, _ok0 := v.(UniOid)"), "{out}");
    assert!(out.contains("if !_ok0 {"), "{out}");
    assert!(out.contains("_c1, err := UniOidToValue(_t0)"), "{out}");
    assert!(out.contains("dst = _c1"), "{out}");

    // arrays type-assert then loop
    let mut tmp = 0;
    let out = go_wire_from_any(
        &UniDataType::Array(Box::new(UniDataType::Scalar(UniScalar::U64))),
        "v",
        "dst",
        &mut tmp,
        "",
        true,
    )?;
    assert!(out.contains("_t0, _ok0 := v.([]uint64)"), "{out}");

    // options unwrap nil
    let mut tmp = 0;
    let out = go_wire_from_any(
        &UniDataType::Option(Box::new(UniDataType::Scalar(UniScalar::U64))),
        "v",
        "dst",
        &mut tmp,
        "",
        true,
    )?;
    assert!(out.contains("if v == nil {"), "{out}");
    assert!(out.contains("dst = nil"), "{out}");

    // tuples check arity, then convert elementwise
    let mut tmp = 0;
    let out = go_wire_from_any(
        &UniDataType::Tuple(vec![
            UniDataType::Scalar(UniScalar::Bool),
            UniDataType::Scalar(UniScalar::String),
        ]),
        "v",
        "dst",
        &mut tmp,
        "",
        true,
    )?;
    assert!(out.contains("_t0, _ok0 := v.([]any)"), "{out}");
    assert!(out.contains("len(_t0) != 2"), "{out}");
    assert!(out.contains("boolChecked(_t0[0])"), "{out}");
    assert!(out.contains("strChecked(_t0[1])"), "{out}");
    assert!(out.contains("dst = _v1"), "{out}");

    // boxes unwrap
    let mut tmp = 0;
    let out = go_wire_from_any(
        &UniDataType::Box(Box::new(UniDataType::Scalar(UniScalar::Bool))),
        "v",
        "dst",
        &mut tmp,
        "",
        true,
    )?;
    assert!(out.contains("boolChecked(v)"), "{out}");
    Ok(())
}
