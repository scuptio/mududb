//! Unit tests for the AssemblyScript front-end.
#![allow(missing_docs)]

use crate::assemblyscript::parser::discover_procedures;
use crate::assemblyscript::procedure::{AsParam, AsProcedure, AsValueType};
use crate::assemblyscript::render::{render_adapter_source, render_wit};
use crate::mtp::main_inner;
use std::error::Error;
use std::path::Path;
use std::time::UNIX_EPOCH;
use tree_sitter::Parser;

#[test]
fn discovers_marked_procedure() -> Result<(), Box<dyn Error>> {
    let code = r#"
        import { Oid, Result, ValueList } from "@mududb/mududb";

        /**mudu-proc*/
        export function transfer(id: Oid, account1: i64, account2: i64): Result<i64> {
          return Result.ok<i64>(0);
        }
    "#;
    let procedures = discover_procedures(code, None)?;
    assert_eq!(
        procedures,
        vec![AsProcedure {
            name: "transfer".to_string(),
            params: vec![
                AsParam {
                    name: "id".to_string(),
                    ty: "Oid".to_string(),
                    value_type: AsValueType::ObjectId,
                },
                AsParam {
                    name: "account1".to_string(),
                    ty: "i64".to_string(),
                    value_type: AsValueType::Int64,
                },
                AsParam {
                    name: "account2".to_string(),
                    ty: "i64".to_string(),
                    value_type: AsValueType::Int64,
                },
            ],
            return_type: "Result<i64>".to_string(),
            return_value_type: AsValueType::Int64,
            returns_result: true,
            id_arg: "id".to_string(),
        }]
    );
    let adapter = render_adapter_source(
        Path::new("procedure.ts"),
        Path::new("procedure.gen.ts"),
        &procedures,
        None,
    )?;
    assert!(
        adapter.contains("export function mp2_transfer(paramPtr: usize, paramLen: usize): usize")
    );
    assert!(adapter.contains("const account1 = values.value(0).asInt64();"));
    assert!(adapter.contains("const account2 = values.value(1).asInt64();"));
    assert!(adapter.contains("const result = __mudu_proc_transfer(id, account1, account2);"));
    assert!(adapter.contains("if (result.isErr)"));
    assert!(adapter.contains("return __muduEncodeProcedureErr(result.unwrapErr(), \"transfer\");"));
    assert!(adapter.contains("returnValues.bind(0, MuduValue.int64(result.unwrap()));"));
    assert!(adapter.contains("return __muduEncodeProcedureOk(returnValues);"));
    assert!(adapter.contains("export function cabi_post_mp2_transfer(result: usize): void {}"));
    let wit = render_wit(&procedures, "test");
    assert!(wit.contains("export mp2-transfer: func(param: list<u8>) -> list<u8>;"));
    assert_typescript_syntax(&adapter)?;
    Ok(())
}

#[test]
fn parses_procedure_signature_and_adapter_uses_original_function() -> Result<(), Box<dyn Error>> {
    let code = r#"
        /**mudu-proc*/
        export function transfer(session: Oid, account: i64): Result<i64> {
          return Result.ok<i64>(account);
        }
    "#;
    let procedures = discover_procedures(code, None)?;
    assert_eq!(procedures[0].name, "transfer");
    assert_eq!(
        procedures[0].params,
        vec![
            AsParam {
                name: "session".to_string(),
                ty: "Oid".to_string(),
                value_type: AsValueType::ObjectId,
            },
            AsParam {
                name: "account".to_string(),
                ty: "i64".to_string(),
                value_type: AsValueType::Int64,
            },
        ]
    );
    assert_eq!(procedures[0].return_type, "Result<i64>");
    assert_eq!(procedures[0].return_value_type, AsValueType::Int64);
    assert!(procedures[0].returns_result);

    let adapter = render_adapter_source(
        Path::new("procedure.ts"),
        Path::new("procedure.gen.ts"),
        &procedures,
        None,
    )?;
    assert!(
        adapter.contains("export function mp2_transfer(paramPtr: usize, paramLen: usize): usize")
    );
    assert!(adapter.contains("const account = values.value(0).asInt64();"));
    assert!(adapter.contains("const result = __mudu_proc_transfer(id, account);"));
    assert!(adapter.contains("returnValues.bind(0, MuduValue.int64(result.unwrap()));"));
    assert!(adapter.contains("return __muduEncodeProcedureErr(result.unwrapErr(), \"transfer\");"));
    assert_typescript_syntax(&adapter)?;
    Ok(())
}

#[test]
fn ignores_unlabeled_function_after_labeled_function() -> Result<(), Box<dyn Error>> {
    let code = r#"
        /**mudu-proc*/
        export function transfer(id: Oid, amount: i64): Result<i64> {
          return Ok(amount);
        }

        export function helper(id: Oid, amount: i64): Result<i64> {
          return Ok(amount);
        }
    "#;
    let procedures = discover_procedures(code, None)?;
    assert_eq!(procedures.len(), 1);
    assert_eq!(procedures[0].name, "transfer");
    Ok(())
}

#[test]
fn rejects_mismatched_procedure_signature() {
    let code = r#"
        /**mudu-proc*/
        export function transfer(id: Oid, values: ValueList): ValueList {
          return values;
        }
    "#;
    assert!(discover_procedures(code, None).is_err());
}

#[test]
fn transpiles_assemblyscript_and_generates_all_artifacts() -> Result<(), Box<dyn Error>> {
    let tmp_pb = mudu_sys::env_var::temp_dir().join(format!(
        "mudu_transpiler_as_{}",
        mudu_sys::time::system_time_now()
            .duration_since(UNIX_EPOCH)?
            .as_nanos()
    ));
    mudu_sys::fs::sync::sync_create_dir_all(&tmp_pb)?;

    let input_path = tmp_pb.join("procedure.ts");
    let output_path = tmp_pb.join("procedure.gen.ts");
    let output_wit_path = tmp_pb.join("procedure.gen.wit");
    let output_proc_desc_path = tmp_pb.join("procedure.desc.json");

    mudu_sys::fs::sync::sync_write(
        &input_path,
        r#"
import { Oid, Result, ValueList } from "@mududb/mududb";

/**mudu-proc*/
export function transfer(id: Oid, account1: i64, account2: i64): Result<i64> {
  return Result.ok<i64>(0);
}
"#,
    )?;

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
        "test",
        "-p",
        output_proc_desc_path.as_str(),
        "-v",
        "assembly-script",
    ];

    let result = main_inner(args);
    assert!(result.is_ok(), "AssemblyScript code");

    let ts = mudu_sys::fs::sync::sync_read_to_string(&output_path)?;
    let wit = mudu_sys::fs::sync::sync_read_to_string(output_wit_path)?;
    let desc = mudu_sys::fs::sync::sync_read_to_string(&output_proc_desc_path)?;

    assert_typescript_syntax(&ts)?;
    assert_wit_syntax(&wit)?;
    let desc_json = serde_json::from_str::<serde_json::Value>(&desc)?;
    let transfer_desc = &desc_json["modules"]["test"][0];
    assert_eq!(transfer_desc["module_name"], "test");
    assert_eq!(transfer_desc["proc_name"], "transfer");
    assert_eq!(transfer_desc["param_desc"]["fields"][0]["name"], "account1");
    assert_eq!(transfer_desc["param_desc"]["fields"][1]["name"], "account2");
    assert_eq!(
        transfer_desc["param_desc"]["fields"][0]["data_type"]["id"],
        "I64"
    );
    assert_eq!(
        transfer_desc["param_desc"]["fields"][1]["data_type"]["id"],
        "I64"
    );
    assert_eq!(transfer_desc["return_desc"]["fields"][0]["name"], "0");
    assert_eq!(
        transfer_desc["return_desc"]["fields"][0]["data_type"]["id"],
        "I64"
    );

    assert!(ts.contains("export function mp2_transfer(paramPtr: usize, paramLen: usize): usize"));
    assert!(ts.contains("const account1 = values.value(0).asInt64();"));
    assert!(ts.contains("const account2 = values.value(1).asInt64();"));
    assert!(ts.contains("const result = __mudu_proc_transfer(id, account1, account2);"));
    assert!(ts.contains("decodeProcedureParam as __muduDecodeProcedureParam"));
    assert!(ts.contains("import {"));
    assert!(ts.contains("transfer as __mudu_proc_transfer"));
    assert!(ts.contains("from \"./procedure\";"));
    assert!(ts.contains("return __muduEncodeProcedureErr(result.unwrapErr(), \"transfer\");"));
    assert!(ts.contains("returnValues.bind(0, MuduValue.int64(result.unwrap()));"));
    assert!(ts.contains("return __muduEncodeProcedureOk(returnValues);"));
    assert!(ts.contains("export function cabi_post_mp2_transfer(result: usize): void {}"));
    assert!(wit.contains("package mududb:test;"));
    assert!(wit.contains("world test {"));
    assert!(wit.contains("import mududb:api/system;"));
    assert!(wit.contains("export mp2-transfer: func(param: list<u8>) -> list<u8>;"));
    assert!(desc.contains("\"transfer\""));
    Ok(())
}

fn assert_typescript_syntax(source: &str) -> Result<(), Box<dyn Error>> {
    let mut parser = Parser::new();
    let language = tree_sitter_typescript::LANGUAGE_TYPESCRIPT;
    parser.set_language(&language.into())?;
    let tree = parser
        .parse(source, None)
        .ok_or("failed to parse TypeScript source")?;
    assert!(
        !tree.root_node().has_error(),
        "generated AssemblyScript/TypeScript syntax should be valid"
    );
    Ok(())
}

// ---- user-defined types (`--type-wit`) ----

use crate::common::type_registry::TypeRegistry;

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

const SHOP_CUSTOM_SOURCE: &str = r#"
import { Oid, Result } from "@mududb/mududb";
import { Profile, Mode } from "./gentypes";

/**mudu-proc*/
export function update_profile(id: Oid, userId: i64, profile: Profile, note: string | null, mode: Mode): Result<Profile> {
  return Result.ok<Profile>(profile);
}
"#;

fn shop_types_registry() -> Result<TypeRegistry, Box<dyn Error>> {
    let dir = mudu_sys::env_var::temp_dir().join(format!(
        "mtp_as_render_types_{}",
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
fn renders_custom_type_wiring() -> Result<(), Box<dyn Error>> {
    let registry = shop_types_registry()?;
    let procedures = discover_procedures(SHOP_CUSTOM_SOURCE, Some(&registry))?;
    assert_eq!(procedures.len(), 1);

    let adapter = render_adapter_source(
        Path::new("procedures.ts"),
        Path::new("procedures.gen.ts"),
        &procedures,
        Some("./gentypes"),
    )?;
    // the generated types module import carries the record codec and the enum
    assert!(adapter.contains("import { ProfileCodec, Mode } from \"./gentypes\";"));
    // record parameter: the record bridge feeds the generated codec
    assert!(adapter.contains(
        "const profile = ProfileCodec.decode(new MpackReader(recordFieldValues(param.param_list[1])));"
    ));
    // option scalar parameter: null guard over the scalar decode
    assert!(adapter.contains(
        "const note: string | null = MuduValue.fromUniDataValue(param.param_list[2]).isNull() ? null : MuduValue.fromUniDataValue(param.param_list[2]).asText();"
    ));
    // enum parameter: the ordinal integer casts to the enum type
    assert!(adapter.contains(
        "const mode = MuduValue.fromUniDataValue(param.param_list[3]).asInt64() as i32 as Mode;"
    ));
    // record result: the generated codec renders the map the record bridge
    // wraps back into the record-case envelope
    assert!(adapter.contains("ProfileCodec.encode(result.unwrap(), __muduResultWriter);"));
    assert!(
        adapter.contains("returnValues.push(recordFromFieldValues(__muduResultWriter.toBytes()));")
    );
    assert!(adapter.contains("return __muduEncodeProcedureOkUni(returnValues);"));
    // the raw-envelope procedure does not build a ValueList
    assert!(!adapter.contains("ValueList.fromUniDataValues(param.param_list)"));
    assert_typescript_syntax(&adapter)?;
    Ok(())
}

#[test]
fn custom_types_require_type_import() -> Result<(), Box<dyn Error>> {
    let registry = shop_types_registry()?;
    let procedures = discover_procedures(SHOP_CUSTOM_SOURCE, Some(&registry))?;
    let err = render_adapter_source(
        Path::new("procedures.ts"),
        Path::new("procedures.gen.ts"),
        &procedures,
        None,
    )
    .err()
    .ok_or("expected a render error")?;
    assert!(
        err.message().contains("--type-import"),
        "error should mention --type-import: {err}"
    );
    Ok(())
}

#[test]
fn transpiles_custom_types_and_generates_all_artifacts() -> Result<(), Box<dyn Error>> {
    let tmp_pb = mudu_sys::env_var::temp_dir().join(format!(
        "mudu_transpiler_as_custom_{}",
        mudu_sys::time::system_time_now()
            .duration_since(UNIX_EPOCH)?
            .as_nanos()
    ));
    mudu_sys::fs::sync::sync_create_dir_all(&tmp_pb)?;

    let input_path = tmp_pb.join("procedures.ts");
    let wit_input_path = tmp_pb.join("types.wit");
    let output_path = tmp_pb.join("procedures.gen.ts");
    let output_wit_path = tmp_pb.join("procedures.gen.wit");
    let output_proc_desc_path = tmp_pb.join("package.desc.json");

    mudu_sys::fs::sync::sync_write(&input_path, SHOP_CUSTOM_SOURCE)?;
    mudu_sys::fs::sync::sync_write(&wit_input_path, SHOP_TYPES_WIT)?;

    let as_str = |path: &Path| {
        path.to_str()
            .map(|s| s.to_string())
            .ok_or("invalid UTF-8 in path")
    };
    let args = vec![
        "mtp".to_string(),
        "-i".to_string(),
        as_str(&input_path)?,
        "-o".to_string(),
        as_str(&output_path)?,
        "-m".to_string(),
        "shop_as".to_string(),
        "-p".to_string(),
        as_str(&output_proc_desc_path)?,
        "-T".to_string(),
        as_str(&wit_input_path)?,
        "--type-import".to_string(),
        "./gentypes".to_string(),
        "assembly-script".to_string(),
    ];
    let result = main_inner(args);
    assert!(
        result.is_ok(),
        "AssemblyScript custom types should transpile: {result:?}"
    );

    let ts = mudu_sys::fs::sync::sync_read_to_string(&output_path)?;
    let wit = mudu_sys::fs::sync::sync_read_to_string(output_wit_path)?;
    let desc = mudu_sys::fs::sync::sync_read_to_string(&output_proc_desc_path)?;

    assert_typescript_syntax(&ts)?;
    assert_wit_syntax(&wit)?;
    assert!(wit.contains("export mp2-update-profile: func(param: list<u8>) -> list<u8>;"));

    let desc_json = serde_json::from_str::<serde_json::Value>(&desc)?;
    let proc_desc = &desc_json["modules"]["shop_as"][0];
    assert_eq!(proc_desc["proc_name"], "update_profile");

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
                    ["zip", string_type]
                ]
            }
        }
    });
    let profile_type = serde_json::json!({
        "id": "Record",
        "param": {
            "Record": {
                "name": "Profile",
                "field": [
                    ["display-name", string_type],
                    ["level", i32_type],
                    ["vip", i32_type],
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
    assert_eq!(
        proc_desc["param_desc"]["fields"][1],
        serde_json::json!({
            "name": "profile",
            "data_type": profile_type,
            "nullable": false
        })
    );
    // the `string | null` option parameter: inner data type plus the
    // nullable flag
    assert_eq!(
        proc_desc["param_desc"]["fields"][2],
        serde_json::json!({
            "name": "note",
            "data_type": string_type,
            "nullable": true
        })
    );
    // the enum parameter rides as i32 ordinals
    assert_eq!(
        proc_desc["param_desc"]["fields"][3],
        serde_json::json!({
            "name": "mode",
            "data_type": i32_type,
            "nullable": false
        })
    );
    assert_eq!(
        proc_desc["return_desc"]["fields"][0],
        serde_json::json!({
            "name": "0",
            "data_type": profile_type,
            "nullable": false
        })
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
    resolve.push_str("procedure.gen.wit", source)?;
    Ok(())
}
