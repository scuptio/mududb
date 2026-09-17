//! Unit tests for the C front-end.
#![allow(missing_docs)]

use crate::c::parser::discover_procedures;
use crate::c::render::render_adapter_source;
use crate::mtp::main_inner;
use std::error::Error;
use std::time::UNIX_EPOCH;
use tree_sitter::Parser;

const SHOP_LIKE_SOURCE: &str = r#"
#include "mudu_sys.h"

// mudu-proc (item_id: i64, name: string) -> i64
int create_item(const mudu_proc_param *param, mudu_datum *result, mudu_error *err) {
    *result = mudu_i64(param->params[0].i64);
    return 0;
}

// mudu-proc (item_id: i64, discount: f64) -> f64
int apply_discount(const mudu_proc_param *param, mudu_datum *result, mudu_error *err) {
    *result = mudu_f64(param->params[1].f64);
    return 0;
}
"#;

#[test]
fn renders_adapter_wrappers_and_checks() -> Result<(), Box<dyn Error>> {
    let procedures = discover_procedures(SHOP_LIKE_SOURCE, None)?;
    assert_eq!(procedures.len(), 2);

    let adapter = render_adapter_source(&procedures, None)?;
    assert!(adapter.contains(
        "int create_item(const mudu_proc_param *param, mudu_datum *result, mudu_error *err);"
    ));
    assert!(adapter.contains(
        "int apply_discount(const mudu_proc_param *param, mudu_datum *result, mudu_error *err);"
    ));
    assert!(adapter.contains(
        "if (param->n_params != 2u || param->params[0].kind != MUDU_DATUM_I64 || param->params[1].kind != MUDU_DATUM_STR) {"
    ));
    assert!(adapter.contains("\"create_item expects (item_id: i64, name: string)\""));
    assert!(adapter.contains(
        "if (param->n_params != 2u || param->params[0].kind != MUDU_DATUM_I64 || param->params[1].kind != MUDU_DATUM_F64) {"
    ));
    assert!(adapter.contains("return create_item(param, result, err);"));
    assert!(adapter.contains("__attribute__((export_name(\"mp2-create-item\"))) uint32_t"));
    assert!(adapter.contains(
        "mp2_create_item(const uint8_t *param, uint32_t param_len) {\n    return mudu_run_proc(param, param_len, mudu_check_create_item);"
    ));
    assert!(adapter.contains("__attribute__((export_name(\"mp2-apply-discount\"))) uint32_t"));
    assert_c_syntax(&adapter)?;
    Ok(())
}

#[test]
fn transpiles_c_and_generates_all_artifacts() -> Result<(), Box<dyn Error>> {
    let tmp_pb = mudu_sys::env_var::temp_dir().join(format!(
        "mudu_transpiler_c_{}",
        mudu_sys::time::system_time_now()
            .duration_since(UNIX_EPOCH)?
            .as_nanos()
    ));
    mudu_sys::fs::sync::sync_create_dir_all(&tmp_pb)?;

    let input_path = tmp_pb.join("procedures.c");
    let output_path = tmp_pb.join("procedures_gen.c");
    let output_wit_path = tmp_pb.join("procedures_gen.wit");
    let output_proc_desc_path = tmp_pb.join("package.desc.json");

    mudu_sys::fs::sync::sync_write(&input_path, SHOP_LIKE_SOURCE)?;

    let input_path = input_path
        .to_str()
        .ok_or("invalid UTF-8 in input path")?
        .to_string();
    let output_path = output_path
        .to_str()
        .ok_or("invalid UTF-8 in output path")?
        .to_string();
    let output_proc_desc_path = output_proc_desc_path
        .to_str()
        .ok_or("invalid UTF-8 in desc path")?
        .to_string();

    let args = vec![
        "mtp",
        "-i",
        input_path.as_str(),
        "-o",
        output_path.as_str(),
        "-m",
        "shop_c",
        "-p",
        output_proc_desc_path.as_str(),
        "-v",
        "c",
    ];

    let result = main_inner(args);
    assert!(result.is_ok(), "C code");

    let c = mudu_sys::fs::sync::sync_read_to_string(&output_path)?;
    let wit = mudu_sys::fs::sync::sync_read_to_string(output_wit_path)?;
    let desc = mudu_sys::fs::sync::sync_read_to_string(&output_proc_desc_path)?;

    assert_c_syntax(&c)?;
    assert_wit_syntax(&wit)?;
    let desc_json = serde_json::from_str::<serde_json::Value>(&desc)?;
    let create_item_desc = &desc_json["modules"]["shop_c"][0];
    assert_eq!(create_item_desc["module_name"], "shop_c");
    assert_eq!(create_item_desc["proc_name"], "create_item");
    assert_eq!(
        create_item_desc["param_desc"]["fields"][0]["name"],
        "item_id"
    );
    assert_eq!(
        create_item_desc["param_desc"]["fields"][0]["data_type"]["id"],
        "I64"
    );
    assert_eq!(
        create_item_desc["param_desc"]["fields"][1]["data_type"]["id"],
        "String"
    );
    assert_eq!(
        create_item_desc["return_desc"]["fields"][0]["data_type"]["id"],
        "I64"
    );
    let discount_desc = &desc_json["modules"]["shop_c"][1];
    assert_eq!(
        discount_desc["param_desc"]["fields"][1]["data_type"]["id"],
        "F64"
    );
    assert_eq!(
        discount_desc["return_desc"]["fields"][0]["data_type"]["id"],
        "F64"
    );

    assert!(c.contains("__attribute__((export_name(\"mp2-create-item\"))) uint32_t"));
    assert!(wit.contains("package mududb:shop-c;"));
    assert!(wit.contains("world shop-c {"));
    assert!(wit.contains("import mududb:api/system;"));
    assert!(wit.contains("export mp2-create-item: func(param: list<u8>) -> list<u8>;"));
    assert!(wit.contains("export mp2-apply-discount: func(param: list<u8>) -> list<u8>;"));
    Ok(())
}

fn assert_c_syntax(source: &str) -> Result<(), Box<dyn Error>> {
    let mut parser = Parser::new();
    let language = tree_sitter_c::LANGUAGE;
    parser.set_language(&language.into())?;
    let tree = parser
        .parse(source, None)
        .ok_or("failed to parse C source")?;
    assert!(
        !tree.root_node().has_error(),
        "generated C syntax should be valid"
    );
    Ok(())
}

// ---- user-defined types (`--type-wit`) ----

use crate::common::type_registry::TypeRegistry;
use std::path::PathBuf;

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
}
"#;

const PROFILE_SOURCE: &str = r#"
#include "mudu_sys.h"

// mudu-proc (user_id: i64, p: Profile, mode: Mode, note: option<string>) -> Profile
int update_profile(const mudu_proc_param *param, mudu_datum *result, mudu_error *err) {
    *result = mudu_i64(param->params[0].i64);
    return 0;
}
"#;

fn shop_types_registry() -> Result<TypeRegistry, Box<dyn Error>> {
    let dir = mudu_sys::env_var::temp_dir().join(format!(
        "mtp_c_tests_types_{}",
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
fn renders_record_enum_and_option_checks() -> Result<(), Box<dyn Error>> {
    let registry = shop_types_registry()?;
    let procedures = discover_procedures(PROFILE_SOURCE, Some(&registry))?;
    assert_eq!(procedures.len(), 1);

    let adapter = render_adapter_source(&procedures, Some("gentypes/Types.h"))?;
    assert!(adapter.contains(
        "if (param->n_params != 4u || param->params[0].kind != MUDU_DATUM_I64 || param->params[1].kind != MUDU_DATUM_RECORD || param->params[2].kind != MUDU_DATUM_I64 || (param->params[3].kind != MUDU_DATUM_STR && param->params[3].kind != MUDU_DATUM_NULL)) {"
    ));
    assert!(adapter.contains(
        "\"update_profile expects (user_id: i64, p: Profile, mode: Mode, note: option<string>)\""
    ));
    // the generated adapter includes the mgen-generated types header named
    // by --type-import
    assert!(adapter.contains("#include \"gentypes/Types.h\""));
    assert!(adapter.contains(
        "int update_profile(const mudu_proc_param *param, mudu_datum *result, mudu_error *err);"
    ));
    assert_c_syntax(&adapter)?;
    Ok(())
}

#[test]
fn custom_types_require_type_import() -> Result<(), Box<dyn Error>> {
    let registry = shop_types_registry()?;
    let procedures = discover_procedures(PROFILE_SOURCE, Some(&registry))?;
    let err = render_adapter_source(&procedures, None)
        .err()
        .ok_or("expected a missing --type-import error")?;
    assert_eq!(err.ec(), mudu::error::ErrorCode::InvalidArgument);
    Ok(())
}

#[test]
fn transpiles_custom_types_and_writes_record_desc() -> Result<(), Box<dyn Error>> {
    let registry = shop_types_registry()?;
    let tmp_pb = mudu_sys::env_var::temp_dir().join(format!(
        "mudu_transpiler_c_types_{}",
        mudu_sys::time::system_time_now()
            .duration_since(UNIX_EPOCH)?
            .as_nanos()
    ));
    mudu_sys::fs::sync::sync_create_dir_all(&tmp_pb)?;

    let input_path = tmp_pb.join("procedures.c");
    let output_path = tmp_pb.join("procedures_gen.c");
    let output_proc_desc_path = tmp_pb.join("package.desc.json");
    mudu_sys::fs::sync::sync_write(&input_path, PROFILE_SOURCE)?;

    let procedures = discover_procedures(
        &mudu_sys::fs::sync::sync_read_to_string(&input_path)?,
        Some(&registry),
    )?;
    let adapter = render_adapter_source(&procedures, Some("gentypes/Types.h"))?;
    mudu_sys::fs::sync::sync_write(&output_path, adapter.as_bytes())?;
    let proc_desc_list = crate::c::desc::gen_procedure_desc_list("shop_c", &procedures);
    crate::common::desc::write_package_desc(
        "shop_c",
        proc_desc_list,
        output_proc_desc_path
            .to_str()
            .ok_or("invalid UTF-8 in desc path")?,
    )?;

    let desc = mudu_sys::fs::sync::sync_read_to_string(&output_proc_desc_path)?;
    let desc_json = serde_json::from_str::<serde_json::Value>(&desc)?;
    let update_desc = &desc_json["modules"]["shop_c"][0];
    // the record parameter: the registry-resolved DataTypeParamRecord with
    // the WIT field names verbatim
    let profile_dt = &update_desc["param_desc"]["fields"][1]["data_type"];
    assert_eq!(profile_dt["id"], "Record");
    assert_eq!(profile_dt["param"]["Record"]["name"], "Profile");
    let record_fields = profile_dt["param"]["Record"]["field"]
        .as_array()
        .ok_or("record fields should be an array")?;
    assert_eq!(record_fields.len(), 5);
    assert_eq!(record_fields[0][0], "display-name");
    assert_eq!(record_fields[0][1]["id"], "String");
    assert_eq!(record_fields[1][0], "level");
    assert_eq!(record_fields[1][1]["id"], "I32");
    assert_eq!(record_fields[2][0], "vip");
    assert_eq!(record_fields[2][1]["id"], "I32");
    assert_eq!(record_fields[3][0], "tags");
    assert_eq!(record_fields[3][1]["id"], "Array");
    assert_eq!(
        record_fields[3][1]["param"]["Array"]["data_type"]["id"],
        "String"
    );
    assert_eq!(record_fields[4][0], "home");
    assert_eq!(record_fields[4][1]["id"], "Record");
    assert_eq!(record_fields[4][1]["param"]["Record"]["name"], "Address");
    // enum parameter: I32 (case ordinals); option parameter: nullable flag
    assert_eq!(
        update_desc["param_desc"]["fields"][2]["data_type"]["id"],
        "I32"
    );
    assert_eq!(
        update_desc["param_desc"]["fields"][3]["data_type"]["id"],
        "String"
    );
    assert_eq!(update_desc["param_desc"]["fields"][3]["nullable"], true);
    // the return descriptor carries the record type too
    assert_eq!(
        update_desc["return_desc"]["fields"][0]["data_type"]["id"],
        "Record"
    );
    assert_eq!(
        update_desc["return_desc"]["fields"][0]["data_type"]["param"]["Record"]["name"],
        "Profile"
    );
    Ok(())
}

// The generated world WIT imports `mududb:api/system` from the host API
// package, so the check resolves it against a stub of that package (mirroring
// the `wit/deps/mududb-api/api.wit` copy the guest builds vendor).
fn assert_wit_syntax(source: &str) -> Result<(), Box<dyn Error>> {
    let mut resolve = wit_parser::Resolve::new();
    resolve.push_str(
        "api.wit",
        "package mududb:api;\n\ninterface system {\n    query: func(query-in: list<u8>) -> list<u8>;\n}\n",
    )?;
    resolve.push_str("procedures_gen.wit", source)?;
    Ok(())
}
