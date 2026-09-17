use super::discover_procedures;
use crate::c::procedure::{CValueType, normalize_type_name};
use mudu::error::ErrorCode;
use std::error::Error;

#[test]
fn discovers_annotated_procedure() -> Result<(), Box<dyn Error>> {
    let code = r#"
#include "mudu_sys.h"

// mudu-proc (item_id: i64, name: string) -> i64
int create_item(const mudu_proc_param *param, mudu_datum *result, mudu_error *err) {
    return 0;
}
"#;
    let procs = discover_procedures(code, None)?;
    assert_eq!(procs.len(), 1);
    assert_eq!(procs[0].name, "create_item");
    assert_eq!(procs[0].params.len(), 2);
    assert_eq!(procs[0].params[0].name, "item_id");
    assert_eq!(procs[0].params[0].value_type, CValueType::Int64);
    assert_eq!(procs[0].params[1].name, "name");
    assert_eq!(procs[0].params[1].value_type, CValueType::Text);
    assert_eq!(procs[0].return_type, "i64");
    assert_eq!(procs[0].return_value_type, CValueType::Int64);
    Ok(())
}

#[test]
fn supports_f64_and_zero_params() -> Result<(), Box<dyn Error>> {
    let code = r#"
// mudu-proc (ratio: f64) -> f64
int scale(const mudu_proc_param *param, mudu_datum *result, mudu_error *err) { return 0; }

// mudu-proc () -> i64
int ping(const mudu_proc_param *param, mudu_datum *result, mudu_error *err) { return 0; }
"#;
    let procs = discover_procedures(code, None)?;
    assert_eq!(procs.len(), 2);
    assert_eq!(procs[0].params[0].value_type, CValueType::Float64);
    assert_eq!(procs[0].return_value_type, CValueType::Float64);
    assert_eq!(procs[1].params.len(), 0);
    Ok(())
}

#[test]
fn ignores_functions_without_label() -> Result<(), Box<dyn Error>> {
    let code = r#"
int not_a_proc(const mudu_proc_param *param, mudu_datum *result, mudu_error *err) { return 0; }
// mudu-proc (x: i64) -> i64
int a_proc(const mudu_proc_param *param, mudu_datum *result, mudu_error *err) { return 0; }
"#;
    let procs = discover_procedures(code, None)?;
    assert_eq!(procs.len(), 1);
    assert_eq!(procs[0].name, "a_proc");
    Ok(())
}

#[test]
fn rejects_syntax_error() -> Result<(), Box<dyn Error>> {
    let code = "int broken( { }";
    let err = discover_procedures(code, None)
        .err()
        .ok_or("expected a parse error")?;
    assert_eq!(err.ec(), ErrorCode::Parse);
    Ok(())
}

#[test]
fn rejects_duplicate_procedure_names() -> Result<(), Box<dyn Error>> {
    let code = r#"
// mudu-proc () -> i64
int dup(const mudu_proc_param *param, mudu_datum *result, mudu_error *err) { return 0; }
// mudu-proc () -> i64
int dup(const mudu_proc_param *param, mudu_datum *result, mudu_error *err) { return 0; }
"#;
    let err = discover_procedures(code, None)
        .err()
        .ok_or("expected a parse error")?;
    assert_eq!(err.ec(), ErrorCode::Parse);
    Ok(())
}

#[test]
fn rejects_static_procedure() -> Result<(), Box<dyn Error>> {
    let code = r#"
// mudu-proc () -> i64
static int hidden(const mudu_proc_param *param, mudu_datum *result, mudu_error *err) { return 0; }
"#;
    let err = discover_procedures(code, None)
        .err()
        .ok_or("expected a parse error")?;
    assert_eq!(err.ec(), ErrorCode::Parse);
    Ok(())
}

#[test]
fn rejects_non_int_return() -> Result<(), Box<dyn Error>> {
    let code = r#"
// mudu-proc () -> i64
void bad(const mudu_proc_param *param, mudu_datum *result, mudu_error *err) { }
"#;
    let err = discover_procedures(code, None)
        .err()
        .ok_or("expected a parse error")?;
    assert_eq!(err.ec(), ErrorCode::Parse);
    Ok(())
}

#[test]
fn rejects_malformed_annotation() -> Result<(), Box<dyn Error>> {
    let code = r#"
// mudu-proc missing-parens
int bad(const mudu_proc_param *param, mudu_datum *result, mudu_error *err) { return 0; }
"#;
    let err = discover_procedures(code, None)
        .err()
        .ok_or("expected a parse error")?;
    assert_eq!(err.ec(), ErrorCode::Parse);
    Ok(())
}

#[test]
fn rejects_annotation_missing_return() -> Result<(), Box<dyn Error>> {
    let code = r#"
// mudu-proc (x: i64)
int bad(const mudu_proc_param *param, mudu_datum *result, mudu_error *err) { return 0; }
"#;
    let err = discover_procedures(code, None)
        .err()
        .ok_or("expected a parse error")?;
    assert_eq!(err.ec(), ErrorCode::Parse);
    Ok(())
}

#[test]
fn rejects_unsupported_annotation_type() -> Result<(), Box<dyn Error>> {
    let code = r#"
// mudu-proc (data: blob) -> i64
int bad(const mudu_proc_param *param, mudu_datum *result, mudu_error *err) { return 0; }
"#;
    let err = discover_procedures(code, None)
        .err()
        .ok_or("expected a parse error")?;
    assert_eq!(err.ec(), ErrorCode::Parse);
    Ok(())
}

#[test]
fn normalize_type_name_trims_and_lowercases() {
    assert_eq!(normalize_type_name("  I64 "), "i64");
    assert_eq!(normalize_type_name("String"), "string");
}

// ---- user-defined types (`--type-wit`) ----

use crate::common::type_registry::TypeRegistry;
use mudu_type::type_family::TypeFamily;
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
        "mtp_c_parser_types_{}",
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
#include "mudu_sys.h"

// mudu-proc (item_id: i64, p: Profile, mode: Mode, note: option<string>) -> Profile
int update_profile(const mudu_proc_param *param, mudu_datum *result, mudu_error *err) {
    return 0;
}
"#;
    let registry = shop_types_registry()?;
    let procs = discover_procedures(code, Some(&registry))?;
    assert_eq!(procs.len(), 1);
    let proc = &procs[0];
    assert_eq!(proc.params.len(), 4);
    assert_eq!(proc.params[1].value_type.datum_kind(), "MUDU_DATUM_RECORD");
    let CValueType::Record(record) = &proc.params[1].value_type else {
        return Err("param 1 should resolve to a record".into());
    };
    assert_eq!(record.name, "Profile");
    assert_eq!(record.data_type.type_family(), TypeFamily::Record);
    let CValueType::Enum(mode) = &proc.params[2].value_type else {
        return Err("param 2 should resolve to an enum".into());
    };
    assert_eq!(mode.name, "Mode");
    assert_eq!(mode.data_type.type_family(), TypeFamily::I32);
    assert_eq!(proc.params[2].value_type.datum_kind(), "MUDU_DATUM_I64");
    let CValueType::Option(inner) = &proc.params[3].value_type else {
        return Err("param 3 should resolve to an option".into());
    };
    assert_eq!(**inner, CValueType::Text);
    assert!(proc.params[3].value_type.is_nullable());
    assert_eq!(proc.params[3].value_type.datum_kind(), "MUDU_DATUM_STR");
    assert_eq!(
        proc.params[3].value_type.data_type().type_family(),
        TypeFamily::String
    );
    let CValueType::Record(return_record) = &proc.return_value_type else {
        return Err("the return type should resolve to a record".into());
    };
    assert_eq!(return_record.name, "Profile");
    Ok(())
}

#[test]
fn custom_type_names_resolve_in_any_case_form() -> Result<(), Box<dyn Error>> {
    let registry = shop_types_registry()?;
    let lower = CValueType::resolve("profile", Some(&registry))?;
    let pascal = CValueType::resolve("Profile", Some(&registry))?;
    assert_eq!(lower, pascal);
    Ok(())
}

#[test]
fn rejects_unknown_custom_type() -> Result<(), Box<dyn Error>> {
    let code = r#"
// mudu-proc (x: Mystery) -> i64
int bad(const mudu_proc_param *param, mudu_datum *result, mudu_error *err) { return 0; }
"#;
    let registry = shop_types_registry()?;
    let err = discover_procedures(code, Some(&registry))
        .err()
        .ok_or("expected a parse error")?;
    assert_eq!(err.ec(), ErrorCode::Parse);
    Ok(())
}

#[test]
fn rejects_custom_type_without_registry() -> Result<(), Box<dyn Error>> {
    let code = r#"
// mudu-proc (x: Profile) -> i64
int bad(const mudu_proc_param *param, mudu_datum *result, mudu_error *err) { return 0; }
"#;
    let err = discover_procedures(code, None)
        .err()
        .ok_or("expected a parse error")?;
    assert_eq!(err.ec(), ErrorCode::Parse);
    Ok(())
}

#[test]
fn rejects_variant_parameter_type() -> Result<(), Box<dyn Error>> {
    let code = r#"
// mudu-proc (s: Shape) -> i64
int bad(const mudu_proc_param *param, mudu_datum *result, mudu_error *err) { return 0; }
"#;
    let registry = shop_types_registry()?;
    let err = discover_procedures(code, Some(&registry))
        .err()
        .ok_or("expected a parse error")?;
    assert_eq!(err.ec(), ErrorCode::Parse);
    Ok(())
}

#[test]
fn rejects_option_return_type() -> Result<(), Box<dyn Error>> {
    let code = r#"
// mudu-proc (x: i64) -> option<Profile>
int bad(const mudu_proc_param *param, mudu_datum *result, mudu_error *err) { return 0; }
"#;
    let registry = shop_types_registry()?;
    let err = discover_procedures(code, Some(&registry))
        .err()
        .ok_or("expected a not-implemented error")?;
    assert_eq!(err.ec(), ErrorCode::NotImplemented);
    Ok(())
}

#[test]
fn rejects_enum_return_type() -> Result<(), Box<dyn Error>> {
    let code = r#"
// mudu-proc (x: i64) -> Mode
int bad(const mudu_proc_param *param, mudu_datum *result, mudu_error *err) { return 0; }
"#;
    let registry = shop_types_registry()?;
    let err = discover_procedures(code, Some(&registry))
        .err()
        .ok_or("expected a not-implemented error")?;
    assert_eq!(err.ec(), ErrorCode::NotImplemented);
    Ok(())
}

#[test]
fn rejects_nested_option_parameter() -> Result<(), Box<dyn Error>> {
    let code = r#"
// mudu-proc (x: option<option<i64>>) -> i64
int bad(const mudu_proc_param *param, mudu_datum *result, mudu_error *err) { return 0; }
"#;
    let err = discover_procedures(code, None)
        .err()
        .ok_or("expected a parse error")?;
    assert_eq!(err.ec(), ErrorCode::Parse);
    Ok(())
}
