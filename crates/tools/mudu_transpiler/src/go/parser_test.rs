use super::discover_procedures;
use crate::go::procedure::{GoValueType, normalize_type_name};
use mudu::error::ErrorCode;
use std::error::Error;

#[test]
fn discovers_marked_procedure() -> Result<(), Box<dyn Error>> {
    let code = r#"
package main

// mudu-proc
func createItem(session muduOid, itemID int64, name string) (int64, error) {
	return itemID, nil
}
"#;
    let procs = discover_procedures(code, None)?;
    assert_eq!(procs.len(), 1);
    assert_eq!(procs[0].name, "create_item");
    assert_eq!(procs[0].func_name, "createItem");
    assert_eq!(procs[0].session_arg, "session");
    assert_eq!(procs[0].params.len(), 3);
    assert_eq!(procs[0].params[0].value_type, GoValueType::ObjectId);
    assert_eq!(procs[0].params[1].value_type, GoValueType::Int64);
    assert_eq!(procs[0].params[2].value_type, GoValueType::Text);
    assert_eq!(procs[0].return_type, "int64");
    assert_eq!(procs[0].return_value_type, GoValueType::Int64);
    Ok(())
}

#[test]
fn supports_the_full_type_table_and_grouped_params() -> Result<(), Box<dyn Error>> {
    let code = r#"
package main

// mudu-proc
func useAll(session muduOid, flag bool, ratio float64, blob []byte, a, b int64) (float64, error) {
	return 0, nil
}
"#;
    let procs = discover_procedures(code, None)?;
    assert_eq!(procs.len(), 1);
    assert_eq!(procs[0].params[1].value_type, GoValueType::Boolean);
    assert_eq!(procs[0].params[2].value_type, GoValueType::Float64);
    assert_eq!(procs[0].params[3].value_type, GoValueType::Binary);
    assert_eq!(procs[0].params[4].name, "a");
    assert_eq!(procs[0].params[5].name, "b");
    assert_eq!(procs[0].return_value_type, GoValueType::Float64);
    Ok(())
}

#[test]
fn ignores_functions_without_label() -> Result<(), Box<dyn Error>> {
    let code = r#"
package main

func notAProc(session muduOid) (int64, error) { return 0, nil }

// mudu-proc
func aProc(session muduOid) (int64, error) { return 0, nil }
"#;
    let procs = discover_procedures(code, None)?;
    assert_eq!(procs.len(), 1);
    assert_eq!(procs[0].func_name, "aProc");
    Ok(())
}

#[test]
fn label_only_applies_to_nearest_following_function() -> Result<(), Box<dyn Error>> {
    let code = r#"
package main

// mudu-proc
func labeled(session muduOid) (int64, error) { return 0, nil }

func unlabeled(session muduOid) (int64, error) { return 0, nil }
"#;
    let procs = discover_procedures(code, None)?;
    assert_eq!(procs.len(), 1);
    assert_eq!(procs[0].func_name, "labeled");
    Ok(())
}

#[test]
fn rejects_syntax_error() -> Result<(), Box<dyn Error>> {
    let code = "package main\nfunc broken( { }";
    let err = discover_procedures(code, None)
        .err()
        .ok_or("expected a parse error")?;
    assert_eq!(err.ec(), ErrorCode::Parse);
    Ok(())
}

#[test]
fn rejects_duplicate_procedure_names() -> Result<(), Box<dyn Error>> {
    let code = r#"
package main

// mudu-proc
func dup(session muduOid) (int64, error) { return 0, nil }

// mudu-proc
func dup(session muduOid) (int64, error) { return 0, nil }
"#;
    let err = discover_procedures(code, None)
        .err()
        .ok_or("expected a parse error")?;
    assert_eq!(err.ec(), ErrorCode::Parse);
    Ok(())
}

#[test]
fn rejects_missing_parameters() -> Result<(), Box<dyn Error>> {
    let code = r#"
package main

// mudu-proc
func noParams() (int64, error) { return 0, nil }
"#;
    let err = discover_procedures(code, None)
        .err()
        .ok_or("expected a parse error")?;
    assert_eq!(err.ec(), ErrorCode::Parse);
    Ok(())
}

#[test]
fn rejects_first_parameter_not_oid() -> Result<(), Box<dyn Error>> {
    let code = r#"
package main

// mudu-proc
func badFirst(name string) (int64, error) { return 0, nil }
"#;
    let err = discover_procedures(code, None)
        .err()
        .ok_or("expected a parse error")?;
    assert_eq!(err.ec(), ErrorCode::Parse);
    Ok(())
}

#[test]
fn rejects_oid_beyond_first_parameter() -> Result<(), Box<dyn Error>> {
    let code = r#"
package main

// mudu-proc
func badOid(session muduOid, other muduOid) (int64, error) { return 0, nil }
"#;
    let err = discover_procedures(code, None)
        .err()
        .ok_or("expected a parse error")?;
    assert_eq!(err.ec(), ErrorCode::Parse);
    Ok(())
}

#[test]
fn rejects_bare_result() -> Result<(), Box<dyn Error>> {
    let code = r#"
package main

// mudu-proc
func bareResult(session muduOid) int64 { return 0 }
"#;
    let err = discover_procedures(code, None)
        .err()
        .ok_or("expected a parse error")?;
    assert_eq!(err.ec(), ErrorCode::Parse);
    Ok(())
}

#[test]
fn rejects_missing_error_result() -> Result<(), Box<dyn Error>> {
    let code = r#"
package main

// mudu-proc
func noError(session muduOid) (int64, int64) { return 0, 0 }
"#;
    let err = discover_procedures(code, None)
        .err()
        .ok_or("expected a parse error")?;
    assert_eq!(err.ec(), ErrorCode::Parse);
    Ok(())
}

#[test]
fn rejects_unsupported_parameter_type() -> Result<(), Box<dyn Error>> {
    let code = r#"
package main

// mudu-proc
func badParam(session muduOid, x int32) (int64, error) { return 0, nil }
"#;
    let err = discover_procedures(code, None)
        .err()
        .ok_or("expected a parse error")?;
    assert_eq!(err.ec(), ErrorCode::Parse);
    Ok(())
}

#[test]
fn rejects_unsupported_result_type() -> Result<(), Box<dyn Error>> {
    let code = r#"
package main

// mudu-proc
func badResult(session muduOid) ([]string, error) { return nil, nil }
"#;
    let err = discover_procedures(code, None)
        .err()
        .ok_or("expected a parse error")?;
    assert_eq!(err.ec(), ErrorCode::Parse);
    Ok(())
}

#[test]
fn normalize_type_name_trims_and_lowercases() {
    assert_eq!(normalize_type_name("  String "), "string");
    assert_eq!(normalize_type_name("[ ] byte"), "[]byte");
}

// ---- user-defined types (`--type-wit`) ----

use crate::common::type_registry::TypeRegistry;
use std::path::PathBuf;
use std::time::UNIX_EPOCH;

const SHOP_TYPES_WIT: &str = r#"
interface shop-types {
    record address {
        city: string,
        zip: string,
    }
    record profile {
        display-name: string,
        level: u32,
        vip: bool,
        tags: list<string>,
        home: option<address>,
    }
    enum mode {
        basic,
        pro,
    }
    variant shape {
        circle(u32),
        point,
    }
}
"#;

fn shop_types_registry() -> Result<TypeRegistry, Box<dyn Error>> {
    let dir = mudu_sys::env_var::temp_dir().join(format!(
        "mtp_go_parser_types_{}",
        mudu_sys::time::system_time_now()
            .duration_since(UNIX_EPOCH)?
            .as_nanos()
    ));
    mudu_sys::fs::sync::sync_create_dir_all(&dir)?;
    let path: PathBuf = dir.join("types.wit");
    mudu_sys::fs::sync::sync_write(&path, SHOP_TYPES_WIT)?;
    Ok(TypeRegistry::from_wit_paths(&[path
        .to_str()
        .ok_or("invalid UTF-8 in path")?
        .to_string()])?)
}

#[test]
fn resolves_record_enum_and_option_parameter_types() -> Result<(), Box<dyn Error>> {
    let code = r#"
package main

// mudu-proc
func updateProfile(session muduOid, id int64, profile gentypes.Profile, note *string, mode gentypes.Mode) (gentypes.Profile, error) {
	return profile, nil
}
"#;
    let registry = shop_types_registry()?;
    let procs = discover_procedures(code, Some(&registry))?;
    assert_eq!(procs.len(), 1);
    assert_eq!(procs[0].params[1].value_type, GoValueType::Int64);
    let GoValueType::Record(profile) = &procs[0].params[2].value_type else {
        return Err(format!(
            "expected a record parameter type, got {:?}",
            procs[0].params[2].value_type
        )
        .into());
    };
    assert_eq!(profile.name, "Profile");
    assert_eq!(
        profile.data_type.type_family(),
        mudu_type::type_family::TypeFamily::Record
    );
    assert!(procs[0].params[3].value_type.is_nullable());
    assert_eq!(
        procs[0].params[3].value_type,
        GoValueType::Option(Box::new(GoValueType::Text))
    );
    let GoValueType::Enum(mode) = &procs[0].params[4].value_type else {
        return Err(format!(
            "expected an enum parameter type, got {:?}",
            procs[0].params[4].value_type
        )
        .into());
    };
    assert_eq!(mode.name, "Mode");
    assert_eq!(
        mode.data_type.type_family(),
        mudu_type::type_family::TypeFamily::I32
    );
    assert!(matches!(procs[0].return_value_type, GoValueType::Record(_)));
    Ok(())
}

#[test]
fn custom_type_requires_type_wit() -> Result<(), Box<dyn Error>> {
    let code = r#"
package main

// mudu-proc
func updateProfile(session muduOid, profile gentypes.Profile) (int64, error) {
	return 0, nil
}
"#;
    let err = discover_procedures(code, None)
        .err()
        .ok_or("expected a parse error")?;
    assert!(
        err.message().contains("--type-wit"),
        "error should mention --type-wit: {err}"
    );
    Ok(())
}

#[test]
fn unknown_custom_type_is_a_clear_error() -> Result<(), Box<dyn Error>> {
    let code = r#"
package main

// mudu-proc
func updateProfile(session muduOid, profile gentypes.Unknown) (int64, error) {
	return 0, nil
}
"#;
    let registry = shop_types_registry()?;
    let err = discover_procedures(code, Some(&registry))
        .err()
        .ok_or("expected a parse error")?;
    assert!(
        err.message().contains("gentypes.Unknown"),
        "error should name the type: {err}"
    );
    Ok(())
}

#[test]
fn rejects_variant_parameter_type() -> Result<(), Box<dyn Error>> {
    let code = r#"
package main

// mudu-proc
func drawShape(session muduOid, shape gentypes.Shape) (int64, error) {
	return 0, nil
}
"#;
    let registry = shop_types_registry()?;
    let err = discover_procedures(code, Some(&registry))
        .err()
        .ok_or("expected a parse error")?;
    assert!(
        err.message().contains("Variant family"),
        "error should explain the variant rejection: {err}"
    );
    Ok(())
}

#[test]
fn rejects_enum_result_type() -> Result<(), Box<dyn Error>> {
    let code = r#"
package main

// mudu-proc
func currentMode(session muduOid) (gentypes.Mode, error) {
	return gentypes.Mode(0), nil
}
"#;
    let registry = shop_types_registry()?;
    let err = discover_procedures(code, Some(&registry))
        .err()
        .ok_or("expected a parse error")?;
    assert!(
        err.message().contains("enum"),
        "error should explain the enum result rejection: {err}"
    );
    Ok(())
}

#[test]
fn rejects_option_result_type() -> Result<(), Box<dyn Error>> {
    let code = r#"
package main

// mudu-proc
func findName(session muduOid) (*string, error) {
	return nil, nil
}
"#;
    let registry = shop_types_registry()?;
    let err = discover_procedures(code, Some(&registry))
        .err()
        .ok_or("expected a parse error")?;
    assert!(
        err.message().contains("option"),
        "error should explain the option result rejection: {err}"
    );
    Ok(())
}

#[test]
fn rejects_nested_option_parameter() -> Result<(), Box<dyn Error>> {
    let code = r#"
package main

// mudu-proc
func badOption(session muduOid, note **string) (int64, error) {
	return 0, nil
}
"#;
    let registry = shop_types_registry()?;
    let err = discover_procedures(code, Some(&registry))
        .err()
        .ok_or("expected a parse error")?;
    assert!(
        err.message().contains("**"),
        "error should explain the nested option rejection: {err}"
    );
    Ok(())
}
