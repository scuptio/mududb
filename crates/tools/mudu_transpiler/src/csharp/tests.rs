//! Unit tests for the C# front-end.
#![allow(missing_docs)]

use crate::csharp::parser::discover_procedures;
use crate::csharp::render::render_adapter_source;
use crate::mtp::main_inner;
use std::error::Error;
use std::time::UNIX_EPOCH;
use tree_sitter::Parser;

const WALLET_LIKE_SOURCE: &str = r#"
#nullable enable

namespace WalletCs;

internal static class Procedures
{
    // mudu-proc
    public static long CreateUser(MuduOid session, long userId, string name, string email)
    {
        return userId;
    }

    // mudu-proc
    public static long TransferFunds(MuduOid session, long fromUserId, long toUserId, long amount)
    {
        return amount;
    }
}
"#;

#[test]
fn renders_exports_class_shape() -> Result<(), Box<dyn Error>> {
    let procedures = discover_procedures(WALLET_LIKE_SOURCE, None)?;
    assert_eq!(procedures.len(), 2);
    assert_eq!(procedures[0].name, "create_user");
    let names = procedures
        .iter()
        .map(|procedure| procedure.name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(names, vec!["create_user", "transfer_funds"]);

    let adapter = render_adapter_source(&procedures, "wallet_cs", None)?;
    assert!(adapter.contains("using WalletCs;"));
    assert!(adapter.contains("namespace WalletCsWorld;"));
    assert!(adapter.contains("public static class WalletCsWorldExportsImpl"));
    assert!(adapter.contains("public static byte[] Mp2CreateUser(byte[] param)"));
    assert!(adapter.contains("public static byte[] Mp2TransferFunds(byte[] param)"));
    assert!(adapter.contains(
        "return Invoke(param, p => Procedures.CreateUser(\n            p.Session, AsI64(p.Params[0]), AsString(p.Params[1]), AsString(p.Params[2])));"
    ));
    assert!(adapter.contains(
        "return Invoke(param, p => Procedures.TransferFunds(\n            p.Session, AsI64(p.Params[0]), AsI64(p.Params[1]), AsI64(p.Params[2])));"
    ));
    assert!(adapter.contains("catch (WalletCsException e)"));
    assert!(adapter.contains("catch (MuduUniException e)"));
    // Only the cast helpers the procedures use are generated.
    assert!(adapter.contains("private static long AsI64(object? value)"));
    assert!(adapter.contains("private static string AsString(object? value)"));
    assert!(!adapter.contains("private static double AsF64(object? value)"));
    assert!(!adapter.contains("private static bool AsBool(object? value)"));
    assert!(!adapter.contains("private static byte[] AsBytes(object? value)"));
    assert_csharp_syntax(&adapter)?;
    Ok(())
}

#[test]
fn renders_full_cast_helper_set_when_used() -> Result<(), Box<dyn Error>> {
    let code = r#"
internal static class Procedures
{
    // mudu-proc
    public static long UseAll(MuduOid session, bool flag, double ratio, byte[] blob, string name)
    {
        return 0L;
    }
}
"#;
    let procedures = discover_procedures(code, None)?;
    let adapter = render_adapter_source(&procedures, "demo", None)?;
    assert!(adapter.contains("AsBool(p.Params[0])"));
    assert!(adapter.contains("AsF64(p.Params[1])"));
    assert!(adapter.contains("AsBytes(p.Params[2])"));
    assert!(adapter.contains("AsString(p.Params[3])"));
    assert!(adapter.contains("private static bool AsBool(object? value)"));
    assert!(adapter.contains("private static double AsF64(object? value)"));
    assert!(adapter.contains("private static byte[] AsBytes(object? value)"));
    assert!(adapter.contains("namespace DemoWorld;"));
    assert!(adapter.contains("catch (DemoException e)"));
    assert_csharp_syntax(&adapter)?;
    Ok(())
}

#[test]
fn transpiles_csharp_and_generates_all_artifacts() -> Result<(), Box<dyn Error>> {
    let tmp_pb = mudu_sys::env_var::temp_dir().join(format!(
        "mudu_transpiler_cs_{}",
        mudu_sys::time::system_time_now()
            .duration_since(UNIX_EPOCH)?
            .as_nanos()
    ));
    mudu_sys::fs::sync::sync_create_dir_all(&tmp_pb)?;

    let input_path = tmp_pb.join("Procedures.cs");
    let output_path = tmp_pb.join("WalletCsWorldExportsImpl.cs");
    let output_wit_path = tmp_pb.join("WalletCsWorldExportsImpl.wit");
    let output_proc_desc_path = tmp_pb.join("package.desc.json");

    mudu_sys::fs::sync::sync_write(&input_path, WALLET_LIKE_SOURCE)?;

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
        "wallet_cs",
        "-p",
        output_proc_desc_path.as_str(),
        "-v",
        "csharp",
    ];

    let result = main_inner(args);
    assert!(result.is_ok(), "C# code");

    let cs = mudu_sys::fs::sync::sync_read_to_string(&output_path)?;
    let wit = mudu_sys::fs::sync::sync_read_to_string(output_wit_path)?;
    let desc = mudu_sys::fs::sync::sync_read_to_string(&output_proc_desc_path)?;

    assert_csharp_syntax(&cs)?;
    assert_wit_syntax(&wit)?;
    let desc_json = serde_json::from_str::<serde_json::Value>(&desc)?;
    let create_user_desc = &desc_json["modules"]["wallet_cs"][0];
    assert_eq!(create_user_desc["module_name"], "wallet_cs");
    assert_eq!(create_user_desc["proc_name"], "create_user");
    assert_eq!(
        create_user_desc["param_desc"]["fields"][0]["name"],
        "user_id"
    );
    assert_eq!(
        create_user_desc["param_desc"]["fields"][0]["data_type"]["id"],
        "I64"
    );
    assert_eq!(
        create_user_desc["param_desc"]["fields"][1]["data_type"]["id"],
        "String"
    );
    assert_eq!(create_user_desc["return_desc"]["fields"][0]["name"], "0");
    assert_eq!(
        create_user_desc["return_desc"]["fields"][0]["data_type"]["id"],
        "I64"
    );

    assert!(cs.contains("public static byte[] Mp2CreateUser(byte[] param)"));
    assert!(wit.contains("package mududb:wallet-cs;"));
    assert!(wit.contains("world wallet-cs {"));
    assert!(wit.contains("import mududb:api/system;"));
    assert!(wit.contains("export mp2-create-user: func(param: list<u8>) -> list<u8>;"));
    assert!(wit.contains("export mp2-transfer-funds: func(param: list<u8>) -> list<u8>;"));
    Ok(())
}

fn assert_csharp_syntax(source: &str) -> Result<(), Box<dyn Error>> {
    let mut parser = Parser::new();
    let language = tree_sitter_c_sharp::LANGUAGE;
    parser.set_language(&language.into())?;
    let tree = parser
        .parse(source, None)
        .ok_or("failed to parse C# source")?;
    assert!(
        !tree.root_node().has_error(),
        "generated C# syntax should be valid"
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
#nullable enable

using WalletCs.Types;

namespace WalletCs;

internal static class Procedures
{
    // mudu-proc
    public static Profile UpdateProfile(MuduOid session, long userId, Profile profile, Mode mode)
    {
        return profile;
    }

    // mudu-proc
    public static string GetName(MuduOid session, long userId)
    {
        return "ada";
    }

    // mudu-proc
    public static long CreateUser(MuduOid session, string name)
    {
        return 1L;
    }
}
"#;

fn shop_types_registry() -> Result<TypeRegistry, Box<dyn Error>> {
    let dir = mudu_sys::env_var::temp_dir().join(format!(
        "mtp_cs_render_types_{}",
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
    assert_eq!(procedures.len(), 3);

    let adapter = render_adapter_source(&procedures, "wallet_cs", Some("WalletCs.Types"))?;
    // record parameter: a per-type helper composes the record bridge with
    // the generated formatter
    assert!(adapter.contains("AsProfile(p.Params[1])"));
    assert!(
        adapter.contains("private static global::WalletCs.Types.Profile AsProfile(object? value)")
    );
    assert!(
        adapter.contains("var reader = new MessagePackReader(MuduSys.RecordFieldValues(value));")
    );
    assert!(adapter.contains(
        "return new global::WalletCs.Types.ProfileFormatter().Deserialize(ref reader, null!);"
    ));
    // enum parameter: the ordinal integer casts to the enum type
    assert!(adapter.contains("(global::WalletCs.Types.Mode)AsI64(p.Params[2])"));
    // record result: the generated formatter renders the map the record
    // bridge wraps back into the record-case envelope
    assert!(adapter.contains(
        "new global::WalletCs.Types.ProfileFormatter().Serialize(ref writer, (global::WalletCs.Types.Profile)value!, null!);"
    ));
    assert!(
        adapter.contains("return MuduSys.EncodeProcedureOkRecord(buffer.WrittenMemory.ToArray());")
    );
    // the lifted scalar result uses an EncodeProcedureOk overload through
    // InvokeObj; the long result keeps the legacy Invoke shape
    assert!(adapter.contains("value => MuduSys.EncodeProcedureOk((string)value!)"));
    assert!(adapter.contains("return InvokeObj(param, p => Procedures.GetName("));
    assert!(adapter.contains(
        "return Invoke(param, p => Procedures.CreateUser(\n            p.Session, AsString(p.Params[0])));"
    ));
    assert!(adapter.contains("using MessagePack;"));
    // the enum parameter draws on the AsI64 cast helper
    assert!(adapter.contains("private static long AsI64(object? value)"));
    assert_csharp_syntax(&adapter)?;
    Ok(())
}

#[test]
fn custom_types_require_type_import() -> Result<(), Box<dyn Error>> {
    let registry = shop_types_registry()?;
    let procedures = discover_procedures(SHOP_CUSTOM_SOURCE, Some(&registry))?;
    let err = render_adapter_source(&procedures, "wallet_cs", None)
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
        "mudu_transpiler_cs_custom_{}",
        mudu_sys::time::system_time_now()
            .duration_since(UNIX_EPOCH)?
            .as_nanos()
    ));
    mudu_sys::fs::sync::sync_create_dir_all(&tmp_pb)?;

    let input_path = tmp_pb.join("Procedures.cs");
    let wit_input_path = tmp_pb.join("types.wit");
    let output_path = tmp_pb.join("WalletCsWorldExportsImpl.cs");
    let output_wit_path = tmp_pb.join("WalletCsWorldExportsImpl.wit");
    let output_proc_desc_path = tmp_pb.join("package.desc.json");

    mudu_sys::fs::sync::sync_write(&input_path, SHOP_CUSTOM_SOURCE)?;
    mudu_sys::fs::sync::sync_write(&wit_input_path, SHOP_TYPES_WIT)?;

    let as_str = |path: &std::path::Path| {
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
        "wallet_cs".to_string(),
        "-p".to_string(),
        as_str(&output_proc_desc_path)?,
        "-T".to_string(),
        as_str(&wit_input_path)?,
        "--type-import".to_string(),
        "WalletCs.Types".to_string(),
        "csharp".to_string(),
    ];
    let result = main_inner(args);
    assert!(
        result.is_ok(),
        "C# custom types should transpile: {result:?}"
    );

    let cs = mudu_sys::fs::sync::sync_read_to_string(&output_path)?;
    let wit = mudu_sys::fs::sync::sync_read_to_string(output_wit_path)?;
    let desc = mudu_sys::fs::sync::sync_read_to_string(&output_proc_desc_path)?;

    assert_csharp_syntax(&cs)?;
    assert_wit_syntax(&wit)?;
    assert!(wit.contains("export mp2-update-profile: func(param: list<u8>) -> list<u8>;"));

    let desc_json = serde_json::from_str::<serde_json::Value>(&desc)?;
    let proc_desc = &desc_json["modules"]["wallet_cs"][0];
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
    // the enum parameter rides as i32 ordinals
    assert_eq!(
        proc_desc["param_desc"]["fields"][2],
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
    // the lifted string result declares the String family
    assert_eq!(
        desc_json["modules"]["wallet_cs"][1]["return_desc"]["fields"][0],
        serde_json::json!({
            "name": "0",
            "data_type": string_type,
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
    resolve.push_str("WalletCsWorldExportsImpl.wit", source)?;
    Ok(())
}
