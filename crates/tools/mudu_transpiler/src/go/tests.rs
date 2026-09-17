//! Unit tests for the Go front-end.
#![allow(missing_docs)]

use crate::go::parser::discover_procedures;
use crate::go::render::render_adapter_source;
use crate::mtp::main_inner;
use std::error::Error;
use std::time::UNIX_EPOCH;
use tree_sitter::Parser;

const SHOP_LIKE_SOURCE: &str = r#"
package main

// mudu-proc
func createItem(session muduOid, itemID int64, name string) (int64, error) {
	return itemID, nil
}

// mudu-proc
func discountPrice(session muduOid, itemID int64, ratio float64) (float64, error) {
	return ratio, nil
}
"#;

#[test]
fn renders_exports_wiring() -> Result<(), Box<dyn Error>> {
    let procedures = discover_procedures(SHOP_LIKE_SOURCE, None)?;
    assert_eq!(procedures.len(), 2);
    assert_eq!(procedures[0].name, "create_item");
    assert_eq!(procedures[1].name, "discount_price");

    let adapter = render_adapter_source(&procedures, "shop_go", None)?;
    assert!(adapter.contains("package main"));
    assert!(adapter.contains("muduworld \"shop_go/binding/mududb/shop-go/shop-go\""));
    assert!(
        adapter.contains(
            "muduworld.Exports.Mp2CreateItem = func(param cm.List[uint8]) cm.List[uint8] {"
        )
    );
    assert!(adapter.contains(
        "muduworld.Exports.Mp2DiscountPrice = func(param cm.List[uint8]) cm.List[uint8] {"
    ));
    assert!(adapter.contains("\"create_item expects 2 parameters (item_id, name)\""));
    assert!(adapter.contains("itemID, err := asI64(p.Params[0])"));
    assert!(adapter.contains("name, err := asString(p.Params[1])"));
    assert!(adapter.contains("result, err := createItem(p.Session, itemID, name)"));
    assert!(adapter.contains("ratio, err := asF64(p.Params[1])"));
    assert!(adapter.contains("func main() {}"));
    assert!(adapter.contains(
        "func invoke(paramBytes []byte, proc func(procedureParam) (any, error)) (result []byte) {"
    ));
    assert_go_syntax(&adapter)?;
    Ok(())
}

#[test]
fn transpiles_go_and_generates_all_artifacts() -> Result<(), Box<dyn Error>> {
    let tmp_pb = mudu_sys::env_var::temp_dir().join(format!(
        "mudu_transpiler_go_{}",
        mudu_sys::time::system_time_now()
            .duration_since(UNIX_EPOCH)?
            .as_nanos()
    ));
    mudu_sys::fs::sync::sync_create_dir_all(&tmp_pb)?;

    let input_path = tmp_pb.join("procedures.go");
    let output_path = tmp_pb.join("main_gen.go");
    let output_wit_path = tmp_pb.join("main_gen.wit");
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
        "shop_go",
        "-p",
        output_proc_desc_path.as_str(),
        "-v",
        "go",
    ];

    let result = main_inner(args);
    assert!(result.is_ok(), "Go code");

    let go = mudu_sys::fs::sync::sync_read_to_string(&output_path)?;
    let wit = mudu_sys::fs::sync::sync_read_to_string(output_wit_path)?;
    let desc = mudu_sys::fs::sync::sync_read_to_string(&output_proc_desc_path)?;

    assert_go_syntax(&go)?;
    assert_wit_syntax(&wit)?;
    let desc_json = serde_json::from_str::<serde_json::Value>(&desc)?;
    let create_item_desc = &desc_json["modules"]["shop_go"][0];
    assert_eq!(create_item_desc["module_name"], "shop_go");
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
    let discount_desc = &desc_json["modules"]["shop_go"][1];
    assert_eq!(discount_desc["proc_name"], "discount_price");
    assert_eq!(
        discount_desc["param_desc"]["fields"][1]["data_type"]["id"],
        "F64"
    );
    assert_eq!(
        discount_desc["return_desc"]["fields"][0]["data_type"]["id"],
        "F64"
    );

    assert!(
        go.contains(
            "muduworld.Exports.Mp2CreateItem = func(param cm.List[uint8]) cm.List[uint8] {"
        )
    );
    assert!(wit.contains("package mududb:shop-go;"));
    assert!(wit.contains("world shop-go {"));
    assert!(wit.contains("    include wasi:cli/imports@0.2.0;\n"));
    assert!(wit.contains("import mududb:api/system;"));
    assert!(wit.contains("export mp2-create-item: func(param: list<u8>) -> list<u8>;"));
    assert!(wit.contains("export mp2-discount-price: func(param: list<u8>) -> list<u8>;"));
    Ok(())
}

fn assert_go_syntax(source: &str) -> Result<(), Box<dyn Error>> {
    let mut parser = Parser::new();
    let language = tree_sitter_go::LANGUAGE;
    parser.set_language(&language.into())?;
    let tree = parser
        .parse(source, None)
        .ok_or("failed to parse Go source")?;
    assert!(
        !tree.root_node().has_error(),
        "generated Go syntax should be valid"
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
package main

// mudu-proc
func updateProfile(session muduOid, id int64, profile gentypes.Profile, note *string, mode gentypes.Mode) (gentypes.Profile, error) {
	return profile, nil
}
"#;

fn shop_types_registry() -> Result<TypeRegistry, Box<dyn Error>> {
    let dir = mudu_sys::env_var::temp_dir().join(format!(
        "mtp_go_types_{}",
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

    let adapter = render_adapter_source(&procedures, "shop_go", Some("shop_go/gentypes"))?;
    assert!(adapter.contains("gentypes \"shop_go/gentypes\""));
    assert!(adapter.contains("\"github.com/ybbh/mududb_p/bindings/go/types\""));
    // record parameter: the record bridge feeds the generated codec
    assert!(adapter.contains("profileFields, err := types.RecordFieldValues(p.Params[1])"));
    assert!(adapter.contains("profile, err := gentypes.ProfileFromValue(profileFields)"));
    // option scalar parameter: nil guard plus address-of the decoded value
    assert!(adapter.contains("var note *string"));
    assert!(adapter.contains("if p.Params[2] != nil {"));
    assert!(adapter.contains("noteValue, err := asString(p.Params[2])"));
    assert!(adapter.contains("note = &noteValue"));
    // enum parameter: the generated codec decodes the ordinal
    assert!(adapter.contains("mode, err := gentypes.ModeFromValue(p.Params[3])"));
    // record result: the generated codec renders the positional map the
    // record bridge wraps back into the record-case envelope
    assert!(adapter.contains("resultWire, err := gentypes.ProfileToValue(result)"));
    assert!(adapter.contains("return types.RecordFromFieldValues(resultWire)"));
    assert!(adapter.contains("result, err := updateProfile(p.Session, id, profile, note, mode)"));
    assert_go_syntax(&adapter)?;
    Ok(())
}

#[test]
fn custom_types_require_type_import() -> Result<(), Box<dyn Error>> {
    let registry = shop_types_registry()?;
    let procedures = discover_procedures(SHOP_CUSTOM_SOURCE, Some(&registry))?;
    let err = render_adapter_source(&procedures, "shop_go", None)
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
        "mudu_transpiler_go_custom_{}",
        mudu_sys::time::system_time_now()
            .duration_since(UNIX_EPOCH)?
            .as_nanos()
    ));
    mudu_sys::fs::sync::sync_create_dir_all(&tmp_pb)?;

    let input_path = tmp_pb.join("procedures.go");
    let wit_input_path = tmp_pb.join("types.wit");
    let output_path = tmp_pb.join("main_gen.go");
    let output_wit_path = tmp_pb.join("main_gen.wit");
    let output_proc_desc_path = tmp_pb.join("package.desc.json");

    mudu_sys::fs::sync::sync_write(&input_path, SHOP_CUSTOM_SOURCE)?;
    mudu_sys::fs::sync::sync_write(&wit_input_path, SHOP_TYPES_WIT)?;

    let as_str = |path: &std::path::Path| {
        path.to_str()
            .map(|s| s.to_string())
            .ok_or("invalid UTF-8 in path")
    };
    let input = as_str(&input_path)?;
    let output = as_str(&output_path)?;
    let desc = as_str(&output_proc_desc_path)?;
    let wit_input = as_str(&wit_input_path)?;
    let args = vec![
        "mtp".to_string(),
        "-i".to_string(),
        input,
        "-o".to_string(),
        output,
        "-m".to_string(),
        "shop_go".to_string(),
        "-p".to_string(),
        desc,
        "-T".to_string(),
        wit_input,
        "--type-import".to_string(),
        "shop_go/gentypes".to_string(),
        "go".to_string(),
    ];
    let result = main_inner(args);
    assert!(
        result.is_ok(),
        "Go custom types should transpile: {result:?}"
    );

    let go = mudu_sys::fs::sync::sync_read_to_string(&output_path)?;
    let wit = mudu_sys::fs::sync::sync_read_to_string(output_wit_path)?;
    let desc = mudu_sys::fs::sync::sync_read_to_string(&output_proc_desc_path)?;

    assert_go_syntax(&go)?;
    assert_wit_syntax(&wit)?;
    assert!(wit.contains("export mp2-update-profile: func(param: list<u8>) -> list<u8>;"));

    let desc_json = serde_json::from_str::<serde_json::Value>(&desc)?;
    let proc_desc = &desc_json["modules"]["shop_go"][0];
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
        proc_desc["param_desc"]["fields"][0],
        serde_json::json!({
            "name": "id",
            "data_type": { "id": "I64", "param": null },
            "nullable": false
        })
    );
    assert_eq!(
        proc_desc["param_desc"]["fields"][1],
        serde_json::json!({
            "name": "profile",
            "data_type": profile_type,
            "nullable": false
        })
    );
    // the *string option parameter: inner data type plus the nullable flag
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
// package and includes `wasi:cli/imports@0.2.0`, so the check resolves it
// against stubs of both packages (mirroring the `wit/deps` copies the guest
// builds vendor).
fn assert_wit_syntax(source: &str) -> Result<(), Box<dyn Error>> {
    let mut resolve = wit_parser::Resolve::new();
    resolve.push_str(
        "api.wit",
        "package mududb:api;\n\ninterface system {\n    query: func(query-in: list<u8>) -> list<u8>;\n}\n",
    )?;
    resolve.push_str(
        "imports.wit",
        "package wasi:cli@0.2.0;\n\nworld imports {}\n",
    )?;
    resolve.push_str("main_gen.wit", source)?;
    Ok(())
}
