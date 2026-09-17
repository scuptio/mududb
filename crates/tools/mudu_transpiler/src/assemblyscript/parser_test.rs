use super::discover_procedures;
use crate::assemblyscript::procedure::{AsValueType, normalize_type_name};
use mudu::error::ErrorCode;
use std::error::Error;

#[test]
fn discovers_exported_procedure_with_oid_first() -> Result<(), Box<dyn Error>> {
    let code = r#"
/**mudu-proc*/
export function getBalance(oid: oid, account: string): i64 {
  return 0;
}
"#;
    let procs = discover_procedures(code, None)?;
    assert_eq!(procs.len(), 1);
    assert_eq!(procs[0].name, "getBalance");
    assert_eq!(procs[0].id_arg, "oid");
    assert_eq!(procs[0].params.len(), 2);
    assert_eq!(procs[0].params[0].value_type, AsValueType::ObjectId);
    assert_eq!(procs[0].params[1].value_type, AsValueType::Text);
    assert_eq!(procs[0].return_value_type, AsValueType::Int64);
    assert!(!procs[0].returns_result);
    Ok(())
}

#[test]
fn result_return_type_sets_returns_result() -> Result<(), Box<dyn Error>> {
    let code = r#"
/**mudu-proc*/
export function tryGet(oid: oid): Result<i64> {
  return { ok: true, value: 0 };
}
"#;
    let procs = discover_procedures(code, None)?;
    assert_eq!(procs.len(), 1);
    assert!(procs[0].returns_result);
    assert_eq!(procs[0].return_value_type, AsValueType::Int64);
    Ok(())
}

#[test]
fn ignores_functions_without_label() -> Result<(), Box<dyn Error>> {
    let code = r#"
function notAProc(oid: oid): i64 { return 0; }
/**mudu-proc*/
export function aProc(oid: oid): i64 { return 0; }
"#;
    let procs = discover_procedures(code, None)?;
    assert_eq!(procs.len(), 1);
    assert_eq!(procs[0].name, "aProc");
    Ok(())
}

#[test]
fn rejects_syntax_error() -> Result<(), Box<dyn Error>> {
    let code = "function broken( { }";
    let err = discover_procedures(code, None)
        .err()
        .ok_or("expected a parse error")?;
    assert_eq!(err.ec(), ErrorCode::Parse);
    Ok(())
}

#[test]
fn rejects_duplicate_procedure_names() -> Result<(), Box<dyn Error>> {
    let code = r#"
/**mudu-proc*/
export function dup(oid: oid): i64 { return 0; }
/**mudu-proc*/
export function dup(oid: oid): i64 { return 0; }
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
/**mudu-proc*/
export function noParams(): i64 { return 0; }
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
/**mudu-proc*/
export function badFirst(name: string): i64 { return 0; }
"#;
    let err = discover_procedures(code, None)
        .err()
        .ok_or("expected a parse error")?;
    assert_eq!(err.ec(), ErrorCode::Parse);
    Ok(())
}

#[test]
fn rejects_missing_return_type() -> Result<(), Box<dyn Error>> {
    let code = r#"
/**mudu-proc*/
export function noReturn(oid: u64) {}
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
/**mudu-proc*/
export function badParam(oid: u64, x: SomeUnknownType): i64 { return 0; }
"#;
    let err = discover_procedures(code, None)
        .err()
        .ok_or("expected a parse error")?;
    assert_eq!(err.ec(), ErrorCode::Parse);
    Ok(())
}

#[test]
fn rejects_unsupported_return_type() -> Result<(), Box<dyn Error>> {
    let code = r#"
/**mudu-proc*/
export function badReturn(oid: u64): SomeUnknownType { return 0 as any; }
"#;
    let err = discover_procedures(code, None)
        .err()
        .ok_or("expected a parse error")?;
    assert_eq!(err.ec(), ErrorCode::Parse);
    Ok(())
}

#[test]
fn rejects_unsupported_result_inner_type() -> Result<(), Box<dyn Error>> {
    let code = r#"
/**mudu-proc*/
export function badResult(oid: u64): Result<SomeUnknownType> { return 0 as any; }
"#;
    let err = discover_procedures(code, None)
        .err()
        .ok_or("expected a parse error")?;
    assert_eq!(err.ec(), ErrorCode::Parse);
    Ok(())
}

#[test]
fn label_only_applies_to_nearest_following_function() -> Result<(), Box<dyn Error>> {
    let code = r#"
/**mudu-proc*/
function labeledButNotExported(oid: oid): i64 { return 0; }
export function exportedWithoutLabel(oid: oid): i64 { return 0; }
"#;
    let procs = discover_procedures(code, None)?;
    assert_eq!(procs.len(), 1);
    assert_eq!(procs[0].name, "labeledButNotExported");
    Ok(())
}

#[test]
fn normalize_type_name_trims_and_lowercases() {
    assert_eq!(normalize_type_name("  :String "), "string");
    assert_eq!(normalize_type_name("Result< i64 >"), "result<i64>");
}

// ---- user-defined types (`--type-wit`) ----

use crate::common::type_registry::TypeRegistry;
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
        circle(f64),
        square(f64),
    }
}
"#;

fn shop_types_registry() -> Result<TypeRegistry, Box<dyn Error>> {
    let dir = mudu_sys::env_var::temp_dir().join(format!(
        "mtp_as_types_{}",
        mudu_sys::time::system_time_now()
            .duration_since(UNIX_EPOCH)?
            .as_nanos()
    ));
    mudu_sys::fs::sync::sync_create_dir_all(&dir)?;
    let path = dir.join("types.wit");
    mudu_sys::fs::sync::sync_write(&path, SHOP_TYPES_WIT)?;
    Ok(TypeRegistry::from_wit_paths(&[path
        .to_str()
        .ok_or("invalid UTF-8 in path")?
        .to_string()])?)
}

#[test]
fn resolves_record_enum_and_option_parameter_types() -> Result<(), Box<dyn Error>> {
    let registry = shop_types_registry()?;
    let code = r#"
/**mudu-proc*/
export function update(oid: oid, profile: Profile, note: string | null, mode: gentypes.Mode): i64 {
  return 0;
}
"#;
    let procs = discover_procedures(code, Some(&registry))?;
    assert_eq!(procs.len(), 1);
    let profile = match &procs[0].params[1].value_type {
        AsValueType::Record(custom) => custom,
        other => return Err(format!("expected a record type, got {other:?}").into()),
    };
    assert_eq!(profile.name, "Profile");
    assert_eq!(
        procs[0].params[2].value_type,
        AsValueType::Option(Box::new(AsValueType::Text))
    );
    assert!(procs[0].params[2].value_type.is_nullable());
    let mode = match &procs[0].params[3].value_type {
        AsValueType::Enum(custom) => custom,
        other => return Err(format!("expected an enum type, got {other:?}").into()),
    };
    assert_eq!(mode.name, "Mode");
    Ok(())
}

#[test]
fn resolves_record_return_type() -> Result<(), Box<dyn Error>> {
    let registry = shop_types_registry()?;
    let code = r#"
/**mudu-proc*/
export function getProfile(oid: oid): Result<Profile> {
  return Result.ok(null);
}
"#;
    let procs = discover_procedures(code, Some(&registry))?;
    assert!(procs[0].returns_result);
    let profile = match &procs[0].return_value_type {
        AsValueType::Record(custom) => custom,
        other => return Err(format!("expected a record return type, got {other:?}").into()),
    };
    assert_eq!(profile.name, "Profile");
    Ok(())
}

#[test]
fn custom_type_without_registry_is_unknown() -> Result<(), Box<dyn Error>> {
    let code = r#"
/**mudu-proc*/
export function update(oid: oid, profile: Profile): i64 { return 0; }
"#;
    let err = discover_procedures(code, None)
        .err()
        .ok_or("expected a parse error")?;
    assert_eq!(err.ec(), ErrorCode::Parse);
    Ok(())
}

#[test]
fn rejects_variant_parameter_type() -> Result<(), Box<dyn Error>> {
    let registry = shop_types_registry()?;
    let code = r#"
/**mudu-proc*/
export function update(oid: oid, shape: Shape): i64 { return 0; }
"#;
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
fn rejects_enum_return_type() -> Result<(), Box<dyn Error>> {
    let registry = shop_types_registry()?;
    let code = r#"
/**mudu-proc*/
export function getMode(oid: oid): Mode { return 0 as any; }
"#;
    let err = discover_procedures(code, Some(&registry))
        .err()
        .ok_or("expected a parse error")?;
    assert!(
        err.message().contains("enum parameters only"),
        "error should explain the enum return rejection: {err}"
    );
    Ok(())
}

#[test]
fn rejects_option_return_type() -> Result<(), Box<dyn Error>> {
    let registry = shop_types_registry()?;
    let code = r#"
/**mudu-proc*/
export function maybeName(oid: oid): string | null { return null; }
"#;
    let err = discover_procedures(code, Some(&registry))
        .err()
        .ok_or("expected a parse error")?;
    assert!(
        err.message().contains("option parameters only"),
        "error should explain the option return rejection: {err}"
    );
    Ok(())
}

#[test]
fn rejects_option_of_oid_parameter() -> Result<(), Box<dyn Error>> {
    let code = r#"
/**mudu-proc*/
export function bad(oid: oid, other: Oid | null): i64 { return 0; }
"#;
    let err = discover_procedures(code, None)
        .err()
        .ok_or("expected a parse error")?;
    assert_eq!(err.ec(), ErrorCode::Parse);
    Ok(())
}

#[test]
fn rejects_option_of_value_types() -> Result<(), Box<dyn Error>> {
    // AssemblyScript allows null only on reference types: `i64 | null` (and
    // bool/f64/enum options) do not exist in the language
    let code = r#"
/**mudu-proc*/
export function bad(oid: oid, count: i64 | null): i64 { return 0; }
"#;
    let err = discover_procedures(code, None)
        .err()
        .ok_or("expected a parse error")?;
    assert!(
        err.message().contains("cannot be nullable"),
        "error should explain the AssemblyScript nullability rule: {err}"
    );
    Ok(())
}

#[test]
fn u128_record_fields_are_unknown_wit_types() -> Result<(), Box<dyn Error>> {
    // WIT scalar keywords stop at 64 bits: `u128` in a record field parses
    // as a type identifier and the registry reports it as unknown (128-bit
    // record fields are therefore unreachable in guests).
    let dir = mudu_sys::env_var::temp_dir().join(format!(
        "mtp_as_u128_types_{}",
        mudu_sys::time::system_time_now()
            .duration_since(UNIX_EPOCH)?
            .as_nanos()
    ));
    mudu_sys::fs::sync::sync_create_dir_all(&dir)?;
    let path = dir.join("types.wit");
    mudu_sys::fs::sync::sync_write(&path, "interface t { record has-oid { owner: u128, } }")?;
    let registry =
        TypeRegistry::from_wit_paths(&[path.to_str().ok_or("invalid UTF-8 in path")?.to_string()])?;
    let code = r#"
/**mudu-proc*/
export function update(oid: oid, value: HasOid): i64 { return 0; }
"#;
    let err = discover_procedures(code, Some(&registry))
        .err()
        .ok_or("expected a parse error")?;
    assert!(
        err.message().contains("unknown WIT type 'u128'"),
        "error should report the unknown u128 field type: {err}"
    );
    Ok(())
}

#[test]
fn rejects_blob_record_fields_in_result_position() -> Result<(), Box<dyn Error>> {
    let dir = mudu_sys::env_var::temp_dir().join(format!(
        "mtp_as_blob_types_{}",
        mudu_sys::time::system_time_now()
            .duration_since(UNIX_EPOCH)?
            .as_nanos()
    ));
    mudu_sys::fs::sync::sync_create_dir_all(&dir)?;
    let path = dir.join("types.wit");
    mudu_sys::fs::sync::sync_write(
        &path,
        "interface t { record doc { title: string, body: blob, } }",
    )?;
    let registry =
        TypeRegistry::from_wit_paths(&[path.to_str().ok_or("invalid UTF-8 in path")?.to_string()])?;
    // parameters are fine: byte-array fields decode cleanly
    let param_code = r#"
/**mudu-proc*/
export function store(oid: oid, doc: Doc): i64 { return 0; }
"#;
    assert!(discover_procedures(param_code, Some(&registry)).is_ok());
    // results are rejected: the generated encoder's homogeneous integer
    // array cannot wrap back into the Binary-family envelope
    let result_code = r#"
/**mudu-proc*/
export function load(oid: oid): Doc { return null as any; }
"#;
    let err = discover_procedures(result_code, Some(&registry))
        .err()
        .ok_or("expected a parse error")?;
    assert!(
        err.message().contains("byte-array"),
        "error should explain the byte-array result rejection: {err}"
    );
    Ok(())
}
