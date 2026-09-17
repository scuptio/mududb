use super::discover_procedures;
use crate::csharp::procedure::{CsValueType, normalize_type_name};
use mudu::error::ErrorCode;
use std::error::Error;

/// C# methods only exist inside a type, so every snippet is wrapped in the
/// same static class the project templates use.
fn wrap(body: &str) -> String {
    format!("internal static class Procedures\n{{\n{body}\n}}\n")
}

#[test]
fn discovers_marked_procedure() -> Result<(), Box<dyn Error>> {
    let code = wrap(
        r#"
    // mudu-proc
    public static long CreateUser(MuduOid session, long userId, string name)
    {
        return userId;
    }
"#,
    );
    let procs = discover_procedures(&code, None)?;
    assert_eq!(procs.len(), 1);
    assert_eq!(procs[0].name, "create_user");
    assert_eq!(procs[0].method_name, "CreateUser");
    assert_eq!(procs[0].session_arg, "session");
    assert_eq!(procs[0].params.len(), 3);
    assert_eq!(procs[0].params[0].value_type, CsValueType::ObjectId);
    assert_eq!(procs[0].params[1].value_type, CsValueType::Int64);
    assert_eq!(procs[0].params[2].value_type, CsValueType::Text);
    assert_eq!(procs[0].return_value_type, CsValueType::Int64);
    Ok(())
}

#[test]
fn supports_the_full_parameter_type_table() -> Result<(), Box<dyn Error>> {
    let code = wrap(
        r#"
    // mudu-proc
    public static long UseAll(MuduOid session, bool flag, double ratio, byte[] blob)
    {
        return 0L;
    }
"#,
    );
    let procs = discover_procedures(&code, None)?;
    assert_eq!(procs.len(), 1);
    assert_eq!(procs[0].params[1].value_type, CsValueType::Boolean);
    assert_eq!(procs[0].params[2].value_type, CsValueType::Float64);
    assert_eq!(procs[0].params[3].value_type, CsValueType::Binary);
    Ok(())
}

#[test]
fn ignores_methods_without_label() -> Result<(), Box<dyn Error>> {
    let code = wrap(
        r#"
    public static long NotAProc(MuduOid session) { return 0L; }
    // mudu-proc
    public static long AProc(MuduOid session) { return 0L; }
"#,
    );
    let procs = discover_procedures(&code, None)?;
    assert_eq!(procs.len(), 1);
    assert_eq!(procs[0].method_name, "AProc");
    Ok(())
}

#[test]
fn label_does_not_cross_other_comments() -> Result<(), Box<dyn Error>> {
    let code = wrap(
        r#"
    // mudu-proc
    // another comment line
    public static long NotPicked(MuduOid session) { return 0L; }
"#,
    );
    let procs = discover_procedures(&code, None)?;
    assert_eq!(procs.len(), 0);
    Ok(())
}

#[test]
fn rejects_syntax_error() -> Result<(), Box<dyn Error>> {
    let code = "class { broken (";
    let err = discover_procedures(code, None)
        .err()
        .ok_or("expected a parse error")?;
    assert_eq!(err.ec(), ErrorCode::Parse);
    Ok(())
}

#[test]
fn rejects_duplicate_procedure_names() -> Result<(), Box<dyn Error>> {
    let code = wrap(
        r#"
    // mudu-proc
    public static long Dup(MuduOid session) { return 0L; }
    // mudu-proc
    public static long Dup(MuduOid session) { return 0L; }
"#,
    );
    let err = discover_procedures(&code, None)
        .err()
        .ok_or("expected a parse error")?;
    assert_eq!(err.ec(), ErrorCode::Parse);
    Ok(())
}

#[test]
fn rejects_missing_parameters() -> Result<(), Box<dyn Error>> {
    let code = wrap(
        r#"
    // mudu-proc
    public static long NoParams() { return 0L; }
"#,
    );
    let err = discover_procedures(&code, None)
        .err()
        .ok_or("expected a parse error")?;
    assert_eq!(err.ec(), ErrorCode::Parse);
    Ok(())
}

#[test]
fn rejects_first_parameter_not_oid() -> Result<(), Box<dyn Error>> {
    let code = wrap(
        r#"
    // mudu-proc
    public static long BadFirst(string name) { return 0L; }
"#,
    );
    let err = discover_procedures(&code, None)
        .err()
        .ok_or("expected a parse error")?;
    assert_eq!(err.ec(), ErrorCode::Parse);
    Ok(())
}

#[test]
fn rejects_oid_beyond_first_parameter() -> Result<(), Box<dyn Error>> {
    let code = wrap(
        r#"
    // mudu-proc
    public static long BadOid(MuduOid session, MuduOid other) { return 0L; }
"#,
    );
    let err = discover_procedures(&code, None)
        .err()
        .ok_or("expected a parse error")?;
    assert_eq!(err.ec(), ErrorCode::Parse);
    Ok(())
}

#[test]
fn rejects_non_static_method() -> Result<(), Box<dyn Error>> {
    let code = wrap(
        r#"
    // mudu-proc
    public long Instance(MuduOid session) { return 0L; }
"#,
    );
    let err = discover_procedures(&code, None)
        .err()
        .ok_or("expected a parse error")?;
    assert_eq!(err.ec(), ErrorCode::Parse);
    Ok(())
}

#[test]
fn rejects_oid_return_type() -> Result<(), Box<dyn Error>> {
    let code = wrap(
        r#"
    // mudu-proc
    public static MuduOid BadReturn(MuduOid session) { return session; }
"#,
    );
    let err = discover_procedures(&code, None)
        .err()
        .ok_or("expected a parse error")?;
    assert_eq!(err.ec(), ErrorCode::Parse);
    Ok(())
}

#[test]
fn accepts_lifted_scalar_return_types() -> Result<(), Box<dyn Error>> {
    // the return side is lifted beyond the historical long-only convention:
    // every scalar but MuduOid is a valid result
    let code = wrap(
        r#"
    // mudu-proc
    public static string GetName(MuduOid session) { return "x"; }
    // mudu-proc
    public static bool GetFlag(MuduOid session) { return true; }
    // mudu-proc
    public static double GetRatio(MuduOid session) { return 0.0; }
    // mudu-proc
    public static byte[] GetBytes(MuduOid session) { return []; }
"#,
    );
    let procs = discover_procedures(&code, None)?;
    assert_eq!(procs.len(), 4);
    assert_eq!(procs[0].return_value_type, CsValueType::Text);
    assert_eq!(procs[1].return_value_type, CsValueType::Boolean);
    assert_eq!(procs[2].return_value_type, CsValueType::Float64);
    assert_eq!(procs[3].return_value_type, CsValueType::Binary);
    Ok(())
}

#[test]
fn rejects_unsupported_parameter_type() -> Result<(), Box<dyn Error>> {
    let code = wrap(
        r#"
    // mudu-proc
    public static long BadParam(MuduOid session, decimal amount) { return 0L; }
"#,
    );
    let err = discover_procedures(&code, None)
        .err()
        .ok_or("expected a parse error")?;
    assert_eq!(err.ec(), ErrorCode::Parse);
    Ok(())
}

#[test]
fn normalize_type_name_trims_and_lowercases() {
    assert_eq!(normalize_type_name("  String "), "string");
    assert_eq!(normalize_type_name("byte [ ]"), "byte[]");
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
        "mtp_cs_types_{}",
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
fn resolves_record_and_enum_parameter_types() -> Result<(), Box<dyn Error>> {
    let registry = shop_types_registry()?;
    let code = wrap(
        r#"
    // mudu-proc
    public static long Update(MuduOid session, Profile profile, WalletCs.Types.Mode mode) { return 0L; }
"#,
    );
    let procs = discover_procedures(&code, Some(&registry))?;
    assert_eq!(procs.len(), 1);
    let profile = match &procs[0].params[1].value_type {
        CsValueType::Record(custom) => custom,
        other => return Err(format!("expected a record type, got {other:?}").into()),
    };
    assert_eq!(profile.name, "Profile");
    let mode = match &procs[0].params[2].value_type {
        CsValueType::Enum(custom) => custom,
        other => return Err(format!("expected an enum type, got {other:?}").into()),
    };
    assert_eq!(mode.name, "Mode");
    Ok(())
}

#[test]
fn resolves_record_and_enum_return_types() -> Result<(), Box<dyn Error>> {
    let registry = shop_types_registry()?;
    let code = wrap(
        r#"
    // mudu-proc
    public static Profile GetProfile(MuduOid session) { return default; }
    // mudu-proc
    public static Mode GetMode(MuduOid session) { return default; }
"#,
    );
    let procs = discover_procedures(&code, Some(&registry))?;
    assert_eq!(procs.len(), 2);
    assert!(matches!(procs[0].return_value_type, CsValueType::Record(_)));
    assert!(matches!(procs[1].return_value_type, CsValueType::Enum(_)));
    Ok(())
}

#[test]
fn custom_type_without_registry_is_unknown() -> Result<(), Box<dyn Error>> {
    let code = wrap(
        r#"
    // mudu-proc
    public static long Update(MuduOid session, Profile profile) { return 0L; }
"#,
    );
    let err = discover_procedures(&code, None)
        .err()
        .ok_or("expected a parse error")?;
    assert_eq!(err.ec(), ErrorCode::Parse);
    Ok(())
}

#[test]
fn rejects_variant_parameter_type() -> Result<(), Box<dyn Error>> {
    let registry = shop_types_registry()?;
    let code = wrap(
        r#"
    // mudu-proc
    public static long Update(MuduOid session, Shape shape) { return 0L; }
"#,
    );
    let err = discover_procedures(&code, Some(&registry))
        .err()
        .ok_or("expected a parse error")?;
    assert!(
        err.message().contains("Variant family"),
        "error should explain the variant rejection: {err}"
    );
    Ok(())
}

#[test]
fn rejects_binary_record_fields() -> Result<(), Box<dyn Error>> {
    // WIT `list<u8>` fields parse as the binary type; the generated C# codec
    // reads them as a MessagePack array of u8 while a blob field reads as
    // bin — the schema-less bridge cannot serve both, so the C# front-end
    // supports blob record fields only.
    let dir = mudu_sys::env_var::temp_dir().join(format!(
        "mtp_cs_binary_types_{}",
        mudu_sys::time::system_time_now()
            .duration_since(UNIX_EPOCH)?
            .as_nanos()
    ));
    mudu_sys::fs::sync::sync_create_dir_all(&dir)?;
    let path = dir.join("types.wit");
    mudu_sys::fs::sync::sync_write(
        &path,
        "interface t { record doc { title: string, body: list<u8>, } }",
    )?;
    let registry =
        TypeRegistry::from_wit_paths(&[path.to_str().ok_or("invalid UTF-8 in path")?.to_string()])?;
    let code = wrap(
        r#"
    // mudu-proc
    public static long Store(MuduOid session, Doc doc) { return 0L; }
"#,
    );
    let err = discover_procedures(&code, Some(&registry))
        .err()
        .ok_or("expected a parse error")?;
    assert!(
        err.message().contains("blob"),
        "error should suggest declaring the field as blob: {err}"
    );
    Ok(())
}
