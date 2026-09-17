//! Unit tests for the WIT type registry (`--type-wit`).
#![allow(missing_docs)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]

use crate::common::desc::{
    ProcDescField, ProcDescModel, gen_procedure_desc_list, write_package_desc,
};
use crate::common::type_registry::{TypeDefKind, TypeRegistry};
use mudu_binding::universal::uni_data_type::UniDataType;
use mudu_binding::universal::uni_record_type::{UniRecordField, UniRecordType};
use mudu_binding::universal::uni_result_type::UniResultType;
use mudu_binding::universal::uni_scalar::UniScalar;
use mudu_type::type_family::TypeFamily;
use std::error::Error;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

const ADDRESS_WIT: &str = r#"
interface address-book {
    record address {
        city: string,
        zip: u32,
    }
}
"#;

const PROFILE_WIT: &str = r#"
interface profile-book {
    record profile {
        display-name: string,
        level: u32,
        tags: list<string>,
        home: address,
    }
}
"#;

const ENUM_WIT: &str = r#"
interface color-book {
    enum color {
        red,
        green,
        blue,
    }
}
"#;

const VARIANT_WIT: &str = r#"
interface shape-book {
    variant shape {
        circle(u32),
        point,
    }
}
"#;

const CYCLE_ALPHA_WIT: &str = r#"
interface cycle-a {
    record alpha {
        beta: beta,
    }
}
"#;

const CYCLE_BETA_WIT: &str = r#"
interface cycle-b {
    record beta {
        alpha: alpha,
    }
}
"#;

const SELF_CYCLE_WIT: &str = r#"
interface node-book {
    record node {
        value: u64,
        next: option<node>,
    }
}
"#;

fn unique_temp_dir(prefix: &str) -> Result<PathBuf, Box<dyn Error>> {
    Ok(mudu_sys::env_var::temp_dir().join(format!(
        "{prefix}_{}",
        mudu_sys::time::system_time_now()
            .duration_since(UNIX_EPOCH)?
            .as_nanos()
    )))
}

/// Write each `(file_name, content)` pair into a fresh temp directory and
/// return the directory path.
fn staged_wit_dir(prefix: &str, files: &[(&str, &str)]) -> Result<PathBuf, Box<dyn Error>> {
    let dir = unique_temp_dir(prefix)?;
    mudu_sys::fs::sync::sync_create_dir_all(&dir)?;
    for (name, content) in files {
        mudu_sys::fs::sync::sync_write(dir.join(name), content)?;
    }
    Ok(dir)
}

fn path_str(path: &Path) -> Result<String, Box<dyn Error>> {
    Ok(path.to_str().ok_or("invalid UTF-8 in path")?.to_string())
}

fn registry_from_dir(prefix: &str, files: &[(&str, &str)]) -> Result<TypeRegistry, Box<dyn Error>> {
    let dir = staged_wit_dir(prefix, files)?;
    Ok(TypeRegistry::from_wit_paths(&[path_str(&dir)?])?)
}

#[test]
fn aggregates_multiple_wit_files() -> Result<(), Box<dyn Error>> {
    let dir = staged_wit_dir(
        "mtp_registry_agg",
        &[
            ("address.wit", ADDRESS_WIT),
            ("profile.wit", PROFILE_WIT),
            ("notes.txt", "not a wit file, ignored"),
        ],
    )?;
    let extra = staged_wit_dir("mtp_registry_agg_extra", &[("color.wit", ENUM_WIT)])?;
    let registry =
        TypeRegistry::from_wit_paths(&[path_str(&dir)?, path_str(&extra.join("color.wit"))?])?;

    assert_eq!(registry.len(), 3);
    assert!(!registry.is_empty());
    // The name resolves in any case form the front-ends may produce.
    for name in ["address", "Address", "ADDRESS"] {
        let def = registry.resolve(name).expect("address should resolve");
        assert!(matches!(def, TypeDefKind::Record(_)));
        assert_eq!(def.declared_name(), "address");
        assert_eq!(def.kind_name(), "record");
    }
    let profile = registry.resolve("Profile").expect("profile should resolve");
    assert!(profile.as_record().is_some());
    let color = registry.resolve("color").expect("color should resolve");
    assert!(matches!(color, TypeDefKind::Enum(_)));
    assert!(color.as_record().is_none());
    assert!(registry.resolve("nope").is_none());
    Ok(())
}

#[test]
fn resolves_identifier_fields_across_files() -> Result<(), Box<dyn Error>> {
    let registry = registry_from_dir(
        "mtp_registry_ident",
        &[("address.wit", ADDRESS_WIT), ("profile.wit", PROFILE_WIT)],
    )?;
    let data_type = registry.to_data_type(&UniDataType::Identifier("profile".to_string()))?;
    assert_eq!(data_type.type_family(), TypeFamily::Record);
    let record = data_type.as_record_param().expect("record param");
    assert_eq!(record.record_name(), "Profile");
    let fields = record.fields();
    assert_eq!(fields.len(), 4);

    assert_eq!(fields[0].0, "display-name");
    assert_eq!(fields[0].1.type_family(), TypeFamily::String);

    assert_eq!(fields[1].0, "level");
    assert_eq!(fields[1].1.type_family(), TypeFamily::I32);

    assert_eq!(fields[2].0, "tags");
    assert_eq!(fields[2].1.type_family(), TypeFamily::Array);
    let tags = fields[2].1.as_array_param().expect("array param");
    assert_eq!(tags.data_type().type_family(), TypeFamily::String);

    assert_eq!(fields[3].0, "home");
    assert_eq!(fields[3].1.type_family(), TypeFamily::Record);
    let home = fields[3].1.as_record_param().expect("nested record param");
    assert_eq!(home.record_name(), "Address");
    assert_eq!(home.fields().len(), 2);
    assert_eq!(home.fields()[0].0, "city");
    assert_eq!(home.fields()[0].1.type_family(), TypeFamily::String);
    assert_eq!(home.fields()[1].0, "zip");
    assert_eq!(home.fields()[1].1.type_family(), TypeFamily::I32);
    Ok(())
}

#[test]
fn unknown_type_name_errors() -> Result<(), Box<dyn Error>> {
    let registry = TypeRegistry::from_wit_paths(&[])?;
    assert!(registry.is_empty());
    let error = registry
        .to_data_type(&UniDataType::Identifier("nope".to_string()))
        .expect_err("unknown name must fail");
    assert!(
        error.to_string().contains("unknown WIT type 'nope'"),
        "{error}"
    );
    Ok(())
}

#[test]
fn duplicate_type_name_errors() -> Result<(), Box<dyn Error>> {
    let other_profile = "interface dup-book {\n    record profile {\n        x: u32,\n    }\n}\n";
    let dir = staged_wit_dir(
        "mtp_registry_dup",
        &[("a.wit", PROFILE_WIT), ("b.wit", other_profile)],
    )?;
    let error = TypeRegistry::from_wit_paths(&[path_str(&dir)?])
        .expect_err("duplicate type name must fail");
    assert!(
        error
            .to_string()
            .contains("duplicate WIT record name 'profile'"),
        "{error}"
    );
    Ok(())
}

#[test]
fn cyclic_records_error_names_the_cycle() -> Result<(), Box<dyn Error>> {
    let dir = staged_wit_dir(
        "mtp_registry_cycle",
        &[("alpha.wit", CYCLE_ALPHA_WIT), ("beta.wit", CYCLE_BETA_WIT)],
    )?;
    let error =
        TypeRegistry::from_wit_paths(&[path_str(&dir)?]).expect_err("cyclic records must fail");
    let message = error.to_string();
    assert!(
        message.contains("cyclic record type reference"),
        "{message}"
    );
    assert!(message.contains("Alpha -> Beta -> Alpha"), "{message}");
    Ok(())
}

#[test]
fn self_referential_record_error_names_the_cycle() -> Result<(), Box<dyn Error>> {
    let dir = staged_wit_dir("mtp_registry_self_cycle", &[("node.wit", SELF_CYCLE_WIT)])?;
    let error =
        TypeRegistry::from_wit_paths(&[path_str(&dir)?]).expect_err("self-reference must fail");
    assert!(error.to_string().contains("Node -> Node"), "{error}");
    Ok(())
}

#[test]
fn variant_type_is_rejected() -> Result<(), Box<dyn Error>> {
    let registry = registry_from_dir("mtp_registry_variant", &[("shape.wit", VARIANT_WIT)])?;
    let def = registry.resolve("shape").expect("shape should resolve");
    assert!(matches!(def, TypeDefKind::Variant(_)));
    let error = registry
        .to_data_type(&UniDataType::Identifier("shape".to_string()))
        .expect_err("variant must be rejected");
    let message = error.to_string();
    assert!(message.contains("variant type 'shape'"), "{message}");
    assert!(message.contains("no Variant family"), "{message}");
    Ok(())
}

#[test]
fn tuple_result_box_types_are_rejected() -> Result<(), Box<dyn Error>> {
    let registry = TypeRegistry::from_wit_paths(&[])?;
    let tuple = UniDataType::Tuple(vec![
        UniDataType::Scalar(UniScalar::U32),
        UniDataType::Scalar(UniScalar::String),
    ]);
    let error = registry
        .to_data_type(&tuple)
        .expect_err("tuple must be rejected");
    assert!(error.to_string().contains("tuple"), "{error}");

    let result = UniDataType::Result(UniResultType {
        ok: Some(Box::new(UniDataType::Scalar(UniScalar::U32))),
        err: None,
    });
    let error = registry
        .to_data_type(&result)
        .expect_err("result must be rejected");
    assert!(error.to_string().contains("result"), "{error}");

    let boxed = UniDataType::Box(Box::new(UniDataType::Scalar(UniScalar::U32)));
    let error = registry
        .to_data_type(&boxed)
        .expect_err("box must be rejected");
    assert!(error.to_string().contains("box"), "{error}");

    // A record holding a tuple field parses, but conversion rejects it.
    let tuple_record_wit =
        "interface pair-book {\n    record pair {\n        xy: tuple<u32, string>,\n    }\n}\n";
    let registry = registry_from_dir("mtp_registry_tuple", &[("pair.wit", tuple_record_wit)])?;
    let error = registry
        .to_data_type(&UniDataType::Identifier("pair".to_string()))
        .expect_err("tuple field must be rejected");
    assert!(error.to_string().contains("tuple"), "{error}");
    Ok(())
}

#[test]
fn scalar_and_composite_mappings() -> Result<(), Box<dyn Error>> {
    let registry = TypeRegistry::from_wit_paths(&[])?;
    let cases: &[(UniScalar, TypeFamily)] = &[
        (UniScalar::Bool, TypeFamily::I32),
        (UniScalar::U8, TypeFamily::I32),
        (UniScalar::I16, TypeFamily::I32),
        (UniScalar::U32, TypeFamily::I32),
        (UniScalar::I32, TypeFamily::I32),
        (UniScalar::U64, TypeFamily::I64),
        (UniScalar::I64, TypeFamily::I64),
        (UniScalar::U128, TypeFamily::U128),
        (UniScalar::I128, TypeFamily::I128),
        (UniScalar::F32, TypeFamily::F32),
        (UniScalar::F64, TypeFamily::F64),
        (UniScalar::String, TypeFamily::String),
        (UniScalar::Blob, TypeFamily::Binary),
        (UniScalar::Numeric, TypeFamily::Numeric),
        (UniScalar::Date, TypeFamily::Date),
    ];
    for (scalar, family) in cases {
        let data_type = registry.to_data_type(&UniDataType::Scalar(*scalar))?;
        assert_eq!(data_type.type_family(), *family, "scalar {scalar:?}");
    }

    // list<u8>/list<i8> arrives as UniDataType::Binary from the WIT parser.
    let binary = registry.to_data_type(&UniDataType::Binary)?;
    assert_eq!(binary.type_family(), TypeFamily::Binary);

    let array = registry.to_data_type(&UniDataType::Array(Box::new(UniDataType::Scalar(
        UniScalar::I64,
    ))))?;
    assert_eq!(array.type_family(), TypeFamily::Array);
    let element = array.as_array_param().expect("array param").data_type();
    assert_eq!(element.type_family(), TypeFamily::I64);

    // Option: nullable reported by to_data_type_nullable, dropped by to_data_type.
    let optional_string = UniDataType::Option(Box::new(UniDataType::Scalar(UniScalar::String)));
    let (data_type, nullable) = registry.to_data_type_nullable(&optional_string)?;
    assert_eq!(data_type.type_family(), TypeFamily::String);
    assert!(nullable);
    let (data_type, nullable) =
        registry.to_data_type_nullable(&UniDataType::Scalar(UniScalar::String))?;
    assert_eq!(data_type.type_family(), TypeFamily::String);
    assert!(!nullable);
    let data_type = registry.to_data_type(&UniDataType::Option(Box::new(UniDataType::Scalar(
        UniScalar::I64,
    ))))?;
    assert_eq!(data_type.type_family(), TypeFamily::I64);

    let error = registry
        .to_data_type(&UniDataType::Scalar(UniScalar::Char))
        .expect_err("char must be rejected");
    assert!(error.to_string().contains("no Char family"), "{error}");

    // Inline (anonymous) record type.
    let inline = UniDataType::Record(UniRecordType {
        record_name: "point".to_string(),
        record_fields: vec![
            UniRecordField {
                field_name: "x".to_string(),
                field_type: UniDataType::Scalar(UniScalar::I32),
                field_attrs: vec![],
            },
            UniRecordField {
                field_name: "y".to_string(),
                field_type: UniDataType::Scalar(UniScalar::I32),
                field_attrs: vec![],
            },
        ],
    });
    let data_type = registry.to_data_type(&inline)?;
    assert_eq!(data_type.type_family(), TypeFamily::Record);
    let record = data_type.as_record_param().expect("record param");
    assert_eq!(record.record_name(), "point");
    assert_eq!(record.fields().len(), 2);
    Ok(())
}

#[test]
fn enum_maps_to_i32() -> Result<(), Box<dyn Error>> {
    let registry = registry_from_dir("mtp_registry_enum", &[("color.wit", ENUM_WIT)])?;
    let data_type = registry.to_data_type(&UniDataType::Identifier("color".to_string()))?;
    assert_eq!(data_type.type_family(), TypeFamily::I32);
    Ok(())
}

#[test]
fn non_wit_file_path_errors() -> Result<(), Box<dyn Error>> {
    let dir = staged_wit_dir("mtp_registry_not_wit", &[("notes.txt", "hello")])?;
    let error = TypeRegistry::from_wit_paths(&[path_str(&dir.join("notes.txt"))?])
        .expect_err("non-WIT file must fail");
    assert!(
        error.to_string().contains("not a .wit file or a directory"),
        "{error}"
    );
    Ok(())
}

#[test]
fn desc_json_snapshot_record_typed_param() -> Result<(), Box<dyn Error>> {
    let registry = registry_from_dir(
        "mtp_registry_snapshot",
        &[("address.wit", ADDRESS_WIT), ("profile.wit", PROFILE_WIT)],
    )?;
    let profile_type = UniDataType::Identifier("profile".to_string());
    let (param_type, nullable) = registry.to_data_type_nullable(&profile_type)?;
    assert!(!nullable);
    // An optional record-typed return maps to the same record DataType plus
    // the nullable flag the caller carries to DatumDesc::new_nullable.
    let (optional_profile, optional_nullable) =
        registry.to_data_type_nullable(&UniDataType::Option(Box::new(profile_type.clone())))?;
    assert!(optional_nullable);
    assert_eq!(optional_profile.type_family(), TypeFamily::Record);

    let model = ProcDescModel {
        name: "set_profile".to_string(),
        argv_fields: vec![ProcDescField {
            name: "p".to_string(),
            data_type: param_type,
            nullable: false,
        }],
        // The nullable flag rides on ProcDescField into
        // DatumDesc::new_nullable; an option<T> field carries it here.
        result_fields: vec![ProcDescField {
            name: "0".to_string(),
            data_type: optional_profile,
            nullable: optional_nullable,
        }],
    };
    let proc_desc_list = gen_procedure_desc_list("wallet", &[model]);
    let dir = unique_temp_dir("mtp_registry_desc")?;
    mudu_sys::fs::sync::sync_create_dir_all(&dir)?;
    let desc_path = dir.join("package.desc.json");
    write_package_desc("wallet", proc_desc_list, &path_str(&desc_path)?)?;

    let json_text = mudu_sys::fs::sync::sync_read_to_string(&desc_path)?;
    let value: serde_json::Value = serde_json::from_str(&json_text)?;
    let string_type = serde_json::json!({
        "id": "String",
        "param": { "String": { "length": 65536 } }
    });
    let i32_type = serde_json::json!({ "id": "I32", "param": null });
    let address_type = serde_json::json!({
        "id": "Record",
        "param": {
            "Record": {
                "name": "Address",
                "field": [
                    ["city", string_type],
                    ["zip", i32_type]
                ]
            }
        }
    });
    let profile_type_json = serde_json::json!({
        "id": "Record",
        "param": {
            "Record": {
                "name": "Profile",
                "field": [
                    ["display-name", string_type],
                    ["level", i32_type],
                    ["tags", {
                        "id": "Array",
                        "param": {
                            "Array": {
                                "data_type": string_type,
                                "max_size": null
                            }
                        }
                    }],
                    ["home", address_type]
                ]
            }
        }
    });
    let expected = serde_json::json!({
        "modules": {
            "wallet": [
                {
                    "module_name": "wallet",
                    "proc_name": "set_profile",
                    "param_desc": {
                        "fields": [
                            {
                                "data_type": profile_type_json,
                                "name": "p",
                                "nullable": false
                            }
                        ]
                    },
                    "return_desc": {
                        "fields": [
                            {
                                "data_type": profile_type_json,
                                "name": "0",
                                "nullable": true
                            }
                        ]
                    }
                }
            ]
        }
    });
    assert_eq!(value, expected);
    Ok(())
}
