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
    let procs = discover_procedures(code)?;
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
    let procs = discover_procedures(code)?;
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
    let procs = discover_procedures(code)?;
    assert_eq!(procs.len(), 1);
    assert_eq!(procs[0].name, "aProc");
    Ok(())
}

#[test]
fn rejects_syntax_error() -> Result<(), Box<dyn Error>> {
    let code = "function broken( { }";
    let err = discover_procedures(code)
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
    let err = discover_procedures(code)
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
    let err = discover_procedures(code)
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
    let err = discover_procedures(code)
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
    let err = discover_procedures(code)
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
    let err = discover_procedures(code)
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
    let err = discover_procedures(code)
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
    let err = discover_procedures(code)
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
    let procs = discover_procedures(code)?;
    assert_eq!(procs.len(), 1);
    assert_eq!(procs[0].name, "labeledButNotExported");
    Ok(())
}

#[test]
fn normalize_type_name_trims_and_lowercases() {
    assert_eq!(normalize_type_name("  :String "), "string");
    assert_eq!(normalize_type_name("Result< i64 >"), "result<i64>");
}
