use crate::assemblyscript::parser::discover_procedures;
use crate::assemblyscript::procedure::{AsParam, AsProcedure, AsValueType};
use crate::assemblyscript::render::{render_adapter_source, render_wit};
use crate::mtp::main_inner;
use std::env::temp_dir;
use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};
use tree_sitter::Parser;

#[test]
fn discovers_marked_procedure() {
    let code = r#"
        import { Oid, Result, ValueList } from "@mududb/assemblyscript-binding";

        /**mudu-proc*/
        export function transfer(id: Oid, account1: i64, account2: i64): Result<i64> {
          return Result.ok<i64>(0);
        }
    "#;
    let procedures = discover_procedures(code).unwrap();
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
    )
    .unwrap();
    assert!(adapter.contains("export function adapter_transfer"));
    assert!(adapter.contains("const account1 = values.value(0).asInt64();"));
    assert!(adapter.contains("const account2 = values.value(1).asInt64();"));
    assert!(adapter.contains("const result = __mudu_proc_transfer(id, account1, account2);"));
    assert!(adapter.contains("if (result.isErr)"));
    assert!(adapter.contains("return __muduProcedureResultErr(result.unwrapErr(), \"transfer\");"));
    assert!(adapter.contains("returnValues.bind(0, MuduValue.int64(result.unwrap()));"));
    assert!(adapter.contains("return __muduProcedureResultOk(returnValues);"));
    let wit = render_wit(&procedures);
    assert!(wit.contains("interface procedure-transfer"));
    assert!(wit.contains("adapter-transfer: func"));
    assert_typescript_syntax(&adapter);
}

#[test]
fn parses_procedure_signature_and_adapter_uses_original_function() {
    let code = r#"
        /**mudu-proc*/
        export function transfer(session: Oid, account: i64): Result<i64> {
          return Result.ok<i64>(account);
        }
    "#;
    let procedures = discover_procedures(code).unwrap();
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
    )
    .unwrap();
    assert!(adapter.contains(
        "export function adapter_transfer(id: Oid, values: ValueList): MuduResult<ValueList>"
    ));
    assert!(adapter.contains("const account = values.value(0).asInt64();"));
    assert!(adapter.contains("const result = __mudu_proc_transfer(id, account);"));
    assert!(adapter.contains("returnValues.bind(0, MuduValue.int64(result.unwrap()));"));
    assert!(adapter.contains("return __muduProcedureResultErr(result.unwrapErr(), \"transfer\");"));
    assert_typescript_syntax(&adapter);
}

#[test]
fn ignores_unlabeled_function_after_labeled_function() {
    let code = r#"
        /**mudu-proc*/
        export function transfer(id: Oid, amount: i64): Result<i64> {
          return Ok(amount);
        }

        export function helper(id: Oid, amount: i64): Result<i64> {
          return Ok(amount);
        }
    "#;
    let procedures = discover_procedures(code).unwrap();
    assert_eq!(procedures.len(), 1);
    assert_eq!(procedures[0].name, "transfer");
}

#[test]
fn rejects_mismatched_procedure_signature() {
    let code = r#"
        /**mudu-proc*/
        export function transfer(id: Oid, values: ValueList): ValueList {
          return values;
        }
    "#;
    assert!(discover_procedures(code).is_err());
}

#[test]
fn transpiles_assemblyscript_and_generates_all_artifacts() {
    let tmp_pb = temp_dir().join(format!(
        "mudu_transpiler_as_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&tmp_pb).unwrap();

    let input_path = tmp_pb.join("procedure.ts");
    let output_path = tmp_pb.join("procedure.gen.ts");
    let output_rust_path = tmp_pb.join("procedure.gen.rs");
    let output_wit_path = tmp_pb.join("procedure.gen.wit");
    let output_proc_desc_path = tmp_pb.join("procedure.desc.json");

    fs::write(
        &input_path,
        r#"
import { Oid, Result, ValueList } from "@mududb/assemblyscript-binding";

/**mudu-proc*/
export function transfer(id: Oid, account1: i64, account2: i64): Result<i64> {
  return Result.ok<i64>(0);
}
"#,
    )
    .unwrap();

    let input_path = input_path.to_str().unwrap().to_string();
    let output_path = output_path.to_str().unwrap().to_string();
    let output_proc_desc_path = output_proc_desc_path.to_str().unwrap().to_string();

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

    let ts = fs::read_to_string(&output_path).unwrap();
    let rs = fs::read_to_string(output_rust_path).unwrap();
    let wit = fs::read_to_string(output_wit_path).unwrap();
    let desc = fs::read_to_string(&output_proc_desc_path).unwrap();

    assert_typescript_syntax(&ts);
    syn::parse_file(&rs).expect("generated Rust wrapper syntax should be valid");
    assert_wit_syntax(&wit);
    serde_json::from_str::<serde_json::Value>(&desc)
        .expect("generated procedure desc JSON syntax should be valid");
    let desc_json = serde_json::from_str::<serde_json::Value>(&desc).unwrap();
    let transfer_desc = &desc_json["modules"]["test"][0];
    assert_eq!(transfer_desc["module_name"], "test");
    assert_eq!(transfer_desc["proc_name"], "transfer");
    assert_eq!(transfer_desc["param_desc"]["fields"][0]["name"], "account1");
    assert_eq!(transfer_desc["param_desc"]["fields"][1]["name"], "account2");
    assert_eq!(
        transfer_desc["param_desc"]["fields"][0]["dat_type"]["id"],
        "I64"
    );
    assert_eq!(
        transfer_desc["param_desc"]["fields"][1]["dat_type"]["id"],
        "I64"
    );
    assert_eq!(transfer_desc["return_desc"]["fields"][0]["name"], "0");
    assert_eq!(
        transfer_desc["return_desc"]["fields"][0]["dat_type"]["id"],
        "I64"
    );

    assert!(ts.contains("export function adapter_transfer"));
    assert!(ts.contains("const account1 = values.value(0).asInt64();"));
    assert!(ts.contains("const account2 = values.value(1).asInt64();"));
    assert!(ts.contains("const result = __mudu_proc_transfer(id, account1, account2);"));
    assert!(ts.contains("procedureResultErr as __muduProcedureResultErr"));
    assert!(ts.contains("import {"));
    assert!(ts.contains("transfer as __mudu_proc_transfer"));
    assert!(ts.contains("from \"./procedure\";"));
    assert!(ts.contains("return __muduProcedureResultErr(result.unwrapErr(), \"transfer\");"));
    assert!(ts.contains("returnValues.bind(0, MuduValue.int64(result.unwrap()));"));
    assert!(ts.contains("return __muduProcedureResultOk(returnValues);"));
    assert!(rs.contains("fn mp2_transfer(param: Vec<u8>) -> Vec<u8>"));
    assert!(rs.contains("pub fn mudu_inner_p2_transfer"));
    assert!(rs.contains("procedure_transfer::adapter_transfer"));
    assert!(rs.contains("DatumDesc::new(\n                \"account1\".to_string(),"));
    assert!(rs.contains("DatTypeID::I64"));
    assert!(wit.contains("interface procedure-transfer"));
    assert!(wit.contains("adapter-transfer: func"));
    assert!(desc.contains("\"transfer\""));
}

fn assert_typescript_syntax(source: &str) {
    let mut parser = Parser::new();
    let language = tree_sitter_typescript::LANGUAGE_TYPESCRIPT;
    parser
        .set_language(&language.into())
        .expect("TypeScript grammar should load");
    let tree = parser
        .parse(source, None)
        .expect("TypeScript source should parse");
    assert!(
        !tree.root_node().has_error(),
        "generated AssemblyScript/TypeScript syntax should be valid"
    );
}

fn assert_wit_syntax(source: &str) {
    let source = source.replacen(
        "package mududb:component-shim;\n\n",
        r#"package mududb:component-shim;

interface types {
    record oid {
        hi: u64,
        lo: u64,
    }

    record error {
        code: u32,
        message: string,
        source: string,
        location: string,
    }
}

interface system {
    use types.{error};

    resource value-list {
        constructor();
        len: func() -> u32;
        value: func(index: u32) -> result<string, error>;
    }
}

"#,
        1,
    );
    wit_parser::UnresolvedPackageGroup::parse_str("procedure.gen.wit", &source)
        .expect("generated WIT syntax should be valid");
}
