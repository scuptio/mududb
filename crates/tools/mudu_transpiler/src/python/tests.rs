//! Unit tests for the Python front-end.
#![allow(missing_docs)]

use crate::mtp::main_inner;
use crate::python::parser::discover_procedures;
use crate::python::render::{render_adapter_source, render_wit};
use crate::python::transpile_python;
use std::error::Error;
use std::path::Path;
use std::time::UNIX_EPOCH;
use tree_sitter::Parser;

const SAMPLE_SOURCE: &str = r#"
# mudu-proc
def transfer(account1: int, account2: int) -> int:
    return 0


# mudu-proc
def greet(name):
    return ("hello", name)


def helper(x):
    return x


# mudu-proc
def create_user(session: UniOid, user_id: int, name: str) -> int:
    return user_id
"#;

#[test]
fn renders_adapter_and_wit() -> Result<(), Box<dyn Error>> {
    let procedures = discover_procedures(SAMPLE_SOURCE, None)?;
    assert_eq!(procedures.len(), 3);

    let adapter = render_adapter_source(Path::new("procedures.py"), &procedures, None)?;
    assert!(adapter.contains("import procedures as procedures"));
    assert!(adapter.contains("class WitWorld(wit_world.WitWorld):"));
    assert!(adapter.contains("def _from_uni(value):"));
    // The syscall transport wiring is installed at module top level so
    // componentize-py bundles `wit_world.imports.system`.
    assert!(adapter.contains("from wit_world.imports import system as _system"));
    assert!(adapter.contains("_mududb_sys.set_transport(_system)"));
    assert!(adapter.contains("except ImportError:"));
    assert!(adapter.contains("def mp2_transfer(self, param: bytes) -> bytes:"));
    assert!(adapter.contains("def mp2_greet(self, param: bytes) -> bytes:"));
    assert!(adapter.contains("def mp2_create_user(self, param: bytes) -> bytes:"));
    assert!(!adapter.contains("def mp2_helper("));
    // Decoded arguments are unwrapped to plain Python values before the call.
    assert!(adapter.contains(
        "result = procedures.transfer(*[_from_uni(_v) for _v in proc_param.param_list])"
    ));
    assert!(
        adapter.contains(
            "result = procedures.greet(*[_from_uni(_v) for _v in proc_param.param_list])"
        )
    );
    // The session-marked procedure gets the bound `UniOid` injected as its
    // first argument (NOT unwrapped); plain procedures splice `param_list`.
    assert!(adapter.contains(
        "result = procedures.create_user(proc_param.session, *[_from_uni(_v) for _v in proc_param.param_list])"
    ));
    assert!(!adapter.contains("procedures.transfer(proc_param.session"));
    assert!(!adapter.contains("procedures.greet(proc_param.session"));
    assert!(adapter.contains("return encode_procedure_ok(_result_values(result))"));
    assert!(adapter.contains("return encode_procedure_err(1, str(e), \"python\", \"transfer\")"));
    assert!(adapter.contains("return encode_procedure_err(1, str(e), \"python\", \"greet\")"));
    assert_python_syntax(&adapter)?;

    let wit = render_wit(&procedures, "wallet_py");
    assert!(wit.contains("package mududb:wallet-py;"));
    assert!(wit.contains("world wallet-py {"));
    assert!(wit.contains("import mududb:api/system;"));
    assert!(wit.contains("export mp2-transfer: func(param: list<u8>) -> list<u8>;"));
    assert!(wit.contains("export mp2-greet: func(param: list<u8>) -> list<u8>;"));
    assert!(wit.contains("export mp2-create-user: func(param: list<u8>) -> list<u8>;"));
    assert_wit_syntax(&wit)?;
    Ok(())
}

#[test]
fn desc_excludes_the_session_parameter() -> Result<(), Box<dyn Error>> {
    let procedures = discover_procedures(SAMPLE_SOURCE, None)?;
    let desc_list = crate::python::desc::gen_procedure_desc_list("test", &procedures);
    assert_eq!(desc_list.len(), 3);
    let create_user = &desc_list[2];
    let field_json = serde_json::to_value(create_user)?;
    assert_eq!(field_json["proc_name"], "create_user");
    let fields = field_json["param_desc"]["fields"]
        .as_array()
        .ok_or("param_desc.fields is not an array")?;
    assert_eq!(fields.len(), 2, "session must be excluded from param_desc");
    assert_eq!(fields[0]["name"], "user_id");
    assert_eq!(fields[0]["data_type"]["id"], "I64");
    assert_eq!(fields[1]["name"], "name");
    assert_eq!(fields[1]["data_type"]["id"], "String");
    // The plain positional procedures keep all their parameters.
    let transfer_fields = serde_json::to_value(&desc_list[0])?["param_desc"]["fields"]
        .as_array()
        .ok_or("param_desc.fields is not an array")?
        .len();
    assert_eq!(transfer_fields, 2);
    Ok(())
}

#[test]
fn transpiles_python_and_generates_all_artifacts() -> Result<(), Box<dyn Error>> {
    let tmp_pb = unique_temp_dir("mudu_transpiler_py")?;
    mudu_sys::fs::sync::sync_create_dir_all(&tmp_pb)?;

    let input_path = tmp_pb.join("procedures.py");
    let output_path = tmp_pb.join("procedures_gen.py");
    let output_wit_path = tmp_pb.join("procedures_gen.wit");
    let output_proc_desc_path = tmp_pb.join("procedures.desc.json");

    mudu_sys::fs::sync::sync_write(&input_path, SAMPLE_SOURCE)?;

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
        "python",
    ];

    let result = main_inner(args);
    assert!(result.is_ok(), "Python code");

    let py = mudu_sys::fs::sync::sync_read_to_string(&output_path)?;
    let wit = mudu_sys::fs::sync::sync_read_to_string(output_wit_path)?;
    let desc = mudu_sys::fs::sync::sync_read_to_string(&output_proc_desc_path)?;

    assert_python_syntax(&py)?;
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
    let greet_desc = &desc_json["modules"]["test"][1];
    assert_eq!(greet_desc["proc_name"], "greet");
    assert_eq!(greet_desc["param_desc"]["fields"][0]["name"], "name");
    // Untyped parameters map to the permissive default (String family).
    assert_eq!(
        greet_desc["param_desc"]["fields"][0]["data_type"]["id"],
        "String"
    );
    // `greet` has no annotation: a single permissive return value.
    assert_eq!(greet_desc["return_desc"]["fields"][0]["name"], "0");
    assert_eq!(
        greet_desc["return_desc"]["fields"][0]["data_type"]["id"],
        "String"
    );
    let create_user_desc = &desc_json["modules"]["test"][2];
    assert_eq!(create_user_desc["proc_name"], "create_user");
    let create_user_fields = create_user_desc["param_desc"]["fields"]
        .as_array()
        .ok_or("param_desc.fields is not an array")?;
    // The bound session parameter is excluded from the wire desc fields.
    assert_eq!(create_user_fields.len(), 2);
    assert_eq!(create_user_fields[0]["name"], "user_id");
    assert_eq!(create_user_fields[0]["data_type"]["id"], "I64");
    assert_eq!(create_user_fields[1]["name"], "name");
    assert_eq!(create_user_fields[1]["data_type"]["id"], "String");
    assert_eq!(
        create_user_desc["return_desc"]["fields"][0]["data_type"]["id"],
        "I64"
    );

    assert!(py.contains("class WitWorld(wit_world.WitWorld):"));
    assert!(py.contains("from wit_world.imports import system as _system"));
    assert!(py.contains("_mududb_sys.set_transport(_system)"));
    assert!(py.contains("def mp2_transfer(self, param: bytes) -> bytes:"));
    assert!(py.contains("def mp2_greet(self, param: bytes) -> bytes:"));
    assert!(py.contains("def mp2_create_user(self, param: bytes) -> bytes:"));
    assert!(py.contains("from mududb.codec.procedure import ("));
    assert!(py.contains(
        "result = procedures.transfer(*[_from_uni(_v) for _v in proc_param.param_list])"
    ));
    assert!(py.contains(
        "result = procedures.create_user(proc_param.session, *[_from_uni(_v) for _v in proc_param.param_list])"
    ));
    assert!(py.contains("return encode_procedure_err(1, str(e), \"python\", \"transfer\")"));
    assert!(wit.contains("package mududb:test;"));
    assert!(wit.contains("world test {"));
    assert!(wit.contains("import mududb:api/system;"));
    assert!(wit.contains("export mp2-transfer: func(param: list<u8>) -> list<u8>;"));
    assert!(wit.contains("export mp2-greet: func(param: list<u8>) -> list<u8>;"));
    assert!(wit.contains("export mp2-create-user: func(param: list<u8>) -> list<u8>;"));
    assert!(desc.contains("\"transfer\""));
    Ok(())
}

/// End-to-end check of the generated adapter in a real CPython interpreter:
/// transpile a fixture module, import the adapter with a minimal `wit_world`
/// stub, invoke the `mp2_*` methods with encoded `UniProcedureParam` bytes,
/// and assert the decoded ok/err arms. Skipped (with a note) when no
/// `python3` interpreter is on PATH.
#[test]
fn generated_adapter_roundtrips_in_cpython() -> Result<(), Box<dyn Error>> {
    if mudu_sys::process::Command::new("python3")
        .arg("--version")
        .output()
        .is_err()
    {
        eprintln!("skipping CPython smoke test: python3 not found");
        return Ok(());
    }

    let tmp_pb = unique_temp_dir("mudu_transpiler_py_smoke")?;
    mudu_sys::fs::sync::sync_create_dir_all(&tmp_pb)?;

    let user_source = r#"
from mududb import sys as mududb_sys
from mududb.generated.uni_oid import UniOid


# mudu-proc
def add_one(x):
    # The adapter unwraps scalar arguments to plain Python values.
    return x + 1 if isinstance(x, int) and not isinstance(x, bool) else -1


# mudu-proc
def greet(name):
    if not isinstance(name, str):
        return ("bad", type(name).__name__)
    return ("hello", name)


# mudu-proc
def boom(x):
    raise ValueError("kaboom")


# mudu-proc
def none_probe(x):
    return 1 if x is None else 0


# mudu-proc
def session_probe(session, x):
    # The session stays a `UniOid` object (NOT unwrapped); `x` is a plain int.
    if isinstance(session, UniOid) and session.h == 0 and session.l == 0:
        return x if isinstance(x, int) and not isinstance(x, bool) else -2
    return -1


# mudu-proc
def open_probe(session):
    # Goes through the `mududb.sys` facade: requires the syscall transport
    # the adapter installs at import time from `wit_world.imports.system`.
    opened = mududb_sys.wit_open("")
    return opened.l
"#;
    let input_path = tmp_pb.join("procedures.py");
    let output_path = tmp_pb.join("procedures_gen.py");
    mudu_sys::fs::sync::sync_write(&input_path, user_source)?;

    let ret = transpile_python(
        &input_path,
        &output_path,
        "smoke".to_string(),
        false,
        None,
        None,
        None,
    );
    assert_eq!(ret, 0, "transpile_python failed");

    // Minimal stand-in for the componentize-py generated bindings module.
    mudu_sys::fs::sync::sync_write(tmp_pb.join("wit_world.py"), "class WitWorld:\n    pass\n")?;

    let driver = r#"
import procedures_gen

from mududb.codec.mpack import MpackReader, MpackWriter
from mududb.generated.uni_data_value import UniDataValueScalar
from mududb.generated.uni_oid import UniOid
from mududb.generated.uni_procedure_param import (
    UniProcedureParam,
    uni_procedure_param_to_value,
)
from mududb.generated.uni_scalar_value import (
    UniScalarValueI64,
    UniScalarValueNull,
    UniScalarValueString,
)


def encode_param(values):
    writer = MpackWriter()
    writer.write_value(
        uni_procedure_param_to_value(
            UniProcedureParam(procedure=0, session=UniOid(), param_list=values)
        )
    )
    return writer.to_bytes()


world = procedures_gen.WitWorld()

ok = world.mp2_add_one(
    encode_param([UniDataValueScalar(inner=UniScalarValueI64(inner=5))])
)
assert MpackReader(ok).read_value() == {0: {1: [[0, [9, 6]]]}}

ok2 = world.mp2_greet(
    encode_param([UniDataValueScalar(inner=UniScalarValueString(inner="world"))])
)
assert MpackReader(ok2).read_value() == {
    0: {1: [[0, [14, "hello"]], [0, [14, "world"]]]}
}

err = world.mp2_boom(
    encode_param([UniDataValueScalar(inner=UniScalarValueI64(inner=1))])
)
assert MpackReader(err).read_value() == {
    1: {1: 1, 2: "kaboom", 3: "python", 4: "boom", 5: []}
}

# The session-marked procedure receives the `UniOid` session object injected
# from `UniProcedureParam.session`; `param_list` carries only `x`.
ok3 = world.mp2_session_probe(
    encode_param([UniDataValueScalar(inner=UniScalarValueI64(inner=7))])
)
assert MpackReader(ok3).read_value() == {0: {1: [[0, [9, 7]]]}}

# A NULL scalar argument unwraps to plain `None`.
ok4 = world.mp2_none_probe(
    encode_param([UniDataValueScalar(inner=UniScalarValueNull())])
)
assert MpackReader(ok4).read_value() == {0: {1: [[0, [9, 1]]]}}

# This scenario stubs `wit_world` as a plain module without an `imports`
# submodule, so the adapter's `try/except ImportError` wiring installs no
# transport and the facade raises on the first syscall.
from mududb import sys as mududb_sys

try:
    mududb_sys.wit_open("")
    raise SystemExit("expected the facade to raise without a transport")
except Exception as e:
    assert "no syscall transport installed" in str(e), e

print("SMOKE-OK")
"#;
    let driver_path = tmp_pb.join("smoke_driver.py");
    mudu_sys::fs::sync::sync_write(&driver_path, driver)?;

    let bindings_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../sdk/bindings/python");
    let python_path = format!(
        "{}:{}",
        tmp_pb.to_str().ok_or("invalid UTF-8 in tmp path")?,
        bindings_path
            .to_str()
            .ok_or("invalid UTF-8 in bindings path")?
    );
    let output = mudu_sys::process::Command::new("python3")
        .arg(&driver_path)
        .env("PYTHONPATH", python_path)
        .output()?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    assert!(
        output.status.success(),
        "CPython smoke test failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(stdout.contains("SMOKE-OK"), "unexpected stdout: {stdout}");

    // Second scenario: `wit_world` as a package carrying a recording
    // `imports.system` stub — the adapter's top-level wiring must install it
    // as the `mududb.sys` transport, so a procedure using the facade hits
    // the stub.
    let tmp_transport = unique_temp_dir("mudu_transpiler_py_smoke_transport")?;
    let imports_dir = tmp_transport.join("wit_world").join("imports");
    mudu_sys::fs::sync::sync_create_dir_all(&imports_dir)?;
    mudu_sys::fs::sync::sync_write(
        tmp_transport.join("wit_world").join("__init__.py"),
        "class WitWorld:\n    pass\n",
    )?;
    mudu_sys::fs::sync::sync_write(imports_dir.join("__init__.py"), "")?;
    mudu_sys::fs::sync::sync_write(
        imports_dir.join("system.py"),
        r#"
from mududb.generated.uni_oid import UniOid
from mududb.generated.uni_syscall import WireResult, encode_open_session_result

calls = []


def open(request):
    calls.append("open")
    return encode_open_session_result(WireResult.ok(UniOid(h=0, l=99)))
"#,
    )?;
    mudu_sys::fs::sync::sync_copy(&output_path, tmp_transport.join("procedures_gen.py"))?;
    mudu_sys::fs::sync::sync_copy(&input_path, tmp_transport.join("procedures.py"))?;

    let transport_driver = r#"
import procedures_gen
import wit_world.imports.system as system_stub

from mududb.codec.mpack import MpackReader, MpackWriter
from mududb.generated.uni_oid import UniOid
from mududb.generated.uni_procedure_param import (
    UniProcedureParam,
    uni_procedure_param_to_value,
)

writer = MpackWriter()
writer.write_value(
    uni_procedure_param_to_value(
        UniProcedureParam(procedure=0, session=UniOid(), param_list=[])
    )
)

world = procedures_gen.WitWorld()
ok = world.mp2_open_probe(writer.to_bytes())
# The facade call inside the procedure hit the stub transport, which
# answered open with the session oid (h=0, l=99).
assert MpackReader(ok).read_value() == {0: {1: [[0, [9, 99]]]}}
assert system_stub.calls == ["open"], system_stub.calls

print("SMOKE-TRANSPORT-OK")
"#;
    let transport_driver_path = tmp_transport.join("smoke_transport_driver.py");
    mudu_sys::fs::sync::sync_write(&transport_driver_path, transport_driver)?;

    let transport_python_path = format!(
        "{}:{}",
        tmp_transport.to_str().ok_or("invalid UTF-8 in tmp path")?,
        bindings_path
            .to_str()
            .ok_or("invalid UTF-8 in bindings path")?
    );
    let output = mudu_sys::process::Command::new("python3")
        .arg(&transport_driver_path)
        .env("PYTHONPATH", transport_python_path)
        .output()?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    assert!(
        output.status.success(),
        "CPython transport smoke test failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("SMOKE-TRANSPORT-OK"),
        "unexpected stdout: {stdout}"
    );

    let _ = mudu_sys::fs::sync::sync_remove_dir_all(&tmp_pb);
    let _ = mudu_sys::fs::sync::sync_remove_dir_all(&tmp_transport);
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
# mudu-proc
def update_profile(session: UniOid, user_id: int, profile: Profile, note: Optional[str], mode: Mode, home: Address | None) -> Profile:
    return profile
"#;

fn shop_types_registry() -> Result<TypeRegistry, Box<dyn Error>> {
    let dir = unique_temp_dir("mtp_py_types")?;
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

    let adapter =
        render_adapter_source(Path::new("procedures.py"), &procedures, Some("shop_types"))?;
    // The generated types module and the record bridge are imported.
    assert!(adapter.contains("\nimport shop_types\n"));
    assert!(adapter.contains("from mududb.codec.bridge import ("));
    assert!(adapter.contains("record_field_values as _record_fields,"));
    assert!(adapter.contains("record_from_field_values as _record_from_fields,"));
    // record parameter: the record bridge feeds the generated codec
    assert!(adapter.contains(
        "profile = shop_types.profile_from_value(_record_fields(proc_param.param_list[1]))"
    ));
    // option scalar parameter: the generic unwrap (Null already maps to None)
    assert!(adapter.contains("note = _from_uni(proc_param.param_list[2])"));
    // enum parameter: the generated codec decodes the ordinal
    assert!(
        adapter.contains("mode = shop_types.mode_from_value(_from_uni(proc_param.param_list[3]))")
    );
    // option record parameter: None guard around the bridge decode
    assert!(adapter.contains(
        "home = None if _from_uni(proc_param.param_list[4]) is None else shop_types.address_from_value(_record_fields(proc_param.param_list[4]))"
    ));
    assert!(adapter.contains(
        "result = procedures.update_profile(proc_param.session, user_id, profile, note, mode, home)"
    ));
    // record result: the generated codec renders the positional dict the
    // record bridge wraps back into the record-case envelope
    assert!(adapter.contains(
        "return encode_procedure_ok([_record_from_fields(shop_types.profile_to_value(result))])"
    ));
    assert_python_syntax(&adapter)?;
    Ok(())
}

#[test]
fn renders_dotted_type_import_under_an_alias() -> Result<(), Box<dyn Error>> {
    let registry = shop_types_registry()?;
    let procedures = discover_procedures(SHOP_CUSTOM_SOURCE, Some(&registry))?;
    let adapter = render_adapter_source(
        Path::new("procedures.py"),
        &procedures,
        Some("shop_types.types"),
    )?;
    assert!(adapter.contains("\nimport shop_types.types as shop_types_types\n"));
    assert!(adapter.contains(
        "profile = shop_types_types.profile_from_value(_record_fields(proc_param.param_list[1]))"
    ));
    assert_python_syntax(&adapter)?;
    Ok(())
}

#[test]
fn custom_types_require_type_import() -> Result<(), Box<dyn Error>> {
    let registry = shop_types_registry()?;
    let procedures = discover_procedures(SHOP_CUSTOM_SOURCE, Some(&registry))?;
    let err = render_adapter_source(Path::new("procedures.py"), &procedures, None)
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
    let tmp_pb = unique_temp_dir("mudu_transpiler_py_custom")?;
    mudu_sys::fs::sync::sync_create_dir_all(&tmp_pb)?;

    let input_path = tmp_pb.join("procedures.py");
    let wit_input_path = tmp_pb.join("types.wit");
    let output_path = tmp_pb.join("procedures_gen.py");
    let output_wit_path = tmp_pb.join("procedures_gen.wit");
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
        "shop_py".to_string(),
        "-p".to_string(),
        desc,
        "-T".to_string(),
        wit_input,
        "--type-import".to_string(),
        "shop_types".to_string(),
        "python".to_string(),
    ];
    let result = main_inner(args);
    assert!(
        result.is_ok(),
        "Python custom types should transpile: {result:?}"
    );

    let py = mudu_sys::fs::sync::sync_read_to_string(&output_path)?;
    let wit = mudu_sys::fs::sync::sync_read_to_string(output_wit_path)?;
    let desc = mudu_sys::fs::sync::sync_read_to_string(&output_proc_desc_path)?;

    assert_python_syntax(&py)?;
    assert_wit_syntax(&wit)?;
    assert!(wit.contains("export mp2-update-profile: func(param: list<u8>) -> list<u8>;"));

    let desc_json = serde_json::from_str::<serde_json::Value>(&desc)?;
    let proc_desc = &desc_json["modules"]["shop_py"][0];
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
    let fields = proc_desc["param_desc"]["fields"]
        .as_array()
        .ok_or("param_desc.fields is not an array")?;
    // The bound session parameter is excluded from the wire desc fields.
    assert_eq!(fields.len(), 5);
    assert_eq!(
        fields[0],
        serde_json::json!({
            "name": "user_id",
            "data_type": { "id": "I64", "param": null },
            "nullable": false
        })
    );
    assert_eq!(
        fields[1],
        serde_json::json!({
            "name": "profile",
            "data_type": profile_type,
            "nullable": false
        })
    );
    // the Optional[str] parameter: inner data type plus the nullable flag
    assert_eq!(
        fields[2],
        serde_json::json!({
            "name": "note",
            "data_type": string_type,
            "nullable": true
        })
    );
    // the enum parameter rides as i32 ordinals
    assert_eq!(
        fields[3],
        serde_json::json!({
            "name": "mode",
            "data_type": i32_type,
            "nullable": false
        })
    );
    // the `Address | None` parameter: the record type plus the nullable flag
    assert_eq!(
        fields[4],
        serde_json::json!({
            "name": "home",
            "data_type": address_type,
            "nullable": true
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

/// End-to-end check of a custom-typed generated adapter in a real CPython
/// interpreter: generate the types module with mgen, transpile a record
/// procedure, and round-trip a record argument/return through the adapter
/// with a minimal `wit_world` stub. Skipped when no `python3` is on PATH.
#[test]
fn custom_type_adapter_roundtrips_in_cpython() -> Result<(), Box<dyn Error>> {
    if mudu_sys::process::Command::new("python3")
        .arg("--version")
        .output()
        .is_err()
    {
        eprintln!("skipping CPython custom-type smoke test: python3 not found");
        return Ok(());
    }

    let tmp_pb = unique_temp_dir("mudu_transpiler_py_smoke_custom")?;
    mudu_sys::fs::sync::sync_create_dir_all(&tmp_pb)?;

    // Generate the project types module exactly like the Makefile task does.
    let wit_input_path = tmp_pb.join("types.wit");
    mudu_sys::fs::sync::sync_write(&wit_input_path, SHOP_TYPES_WIT)?;
    mudu_gen::src_gen::gen_message::gen_message(
        &wit_input_path,
        tmp_pb.join("shop_types.py"),
        "python".to_string(),
        None,
    )?;

    let user_source = r#"
from mududb.generated.uni_oid import UniOid
from shop_types import Profile


# mudu-proc
def update_profile(session: UniOid, user_id: int, profile: Profile) -> Profile:
    # The record argument is a typed `Profile` dataclass instance.
    assert isinstance(profile, Profile), type(profile)
    profile.display_name = f"{profile.display_name}#{user_id}"
    return profile
"#;
    let input_path = tmp_pb.join("procedures.py");
    let output_path = tmp_pb.join("procedures_gen.py");
    mudu_sys::fs::sync::sync_write(&input_path, user_source)?;

    let registry = TypeRegistry::from_wit_paths(&[wit_input_path
        .to_str()
        .ok_or("invalid UTF-8 in path")?
        .to_string()])?;
    let ret = transpile_python(
        &input_path,
        &output_path,
        "smoke".to_string(),
        false,
        None,
        Some(&registry),
        Some("shop_types".to_string()),
    );
    assert_eq!(ret, 0, "transpile_python failed");

    // Minimal stand-in for the componentize-py generated bindings module.
    mudu_sys::fs::sync::sync_write(tmp_pb.join("wit_world.py"), "class WitWorld:\n    pass\n")?;

    let driver = r#"
import procedures_gen

from mududb.codec.bridge import record_field_values, record_from_field_values
from mududb.codec.mpack import MpackReader, MpackWriter
from mududb.generated.uni_data_value import (
    UniDataValueScalar,
    uni_data_value_from_value,
)
from mududb.generated.uni_oid import UniOid
from mududb.generated.uni_procedure_param import (
    UniProcedureParam,
    uni_procedure_param_to_value,
)
from mududb.generated.uni_scalar_value import UniScalarValueI64
from shop_types import profile_from_value


def encode_param(values):
    writer = MpackWriter()
    writer.write_value(
        uni_procedure_param_to_value(
            UniProcedureParam(procedure=0, session=UniOid(), param_list=values)
        )
    )
    return writer.to_bytes()


# The host sends the record case positionally: names empty, the WIT bool
# field as the I32 0/1 form (its vocabulary has no Bool case).
arg = record_from_field_values(
    {1: "Ada", 2: 7, 3: True, 4: ["a", "b"], 5: {1: "sh", 2: "200"}}
)
assert arg.inner[2].field_value.inner.inner == 1  # vip rides as I32(1)

world = procedures_gen.WitWorld()
ok = world.mp2_update_profile(
    encode_param([UniDataValueScalar(inner=UniScalarValueI64(inner=1)), arg])
)
result = MpackReader(ok).read_value()
assert set(result.keys()) == {0}, result
record_uv = uni_data_value_from_value(result[0][1][0])
stored = profile_from_value(record_field_values(record_uv))
assert stored.display_name == "Ada#1", stored
assert stored.level == 7 and stored.vip is True and stored.tags == ["a", "b"], stored
assert stored.home is not None and stored.home.city == "sh" and stored.home.zip == "200", stored.home
# The record return rides as the positional record case: empty names, the
# bool field back as I32(1), integers as I64 (the bridge's encode mapping).
fields = result[0][1][0][1]
assert fields[0] == {1: "", 2: [0, [14, "Ada#1"]]}, fields[0]
assert fields[1] == {1: "", 2: [0, [9, 7]]}, fields[1]
assert fields[2] == {1: "", 2: [0, [6, 1]]}, fields[2]

print("SMOKE-CUSTOM-OK")
"#;
    let driver_path = tmp_pb.join("smoke_custom_driver.py");
    mudu_sys::fs::sync::sync_write(&driver_path, driver)?;

    let bindings_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../sdk/bindings/python");
    let python_path = format!(
        "{}:{}",
        tmp_pb.to_str().ok_or("invalid UTF-8 in tmp path")?,
        bindings_path
            .to_str()
            .ok_or("invalid UTF-8 in bindings path")?
    );
    let output = mudu_sys::process::Command::new("python3")
        .arg(&driver_path)
        .env("PYTHONPATH", python_path)
        .output()?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    assert!(
        output.status.success(),
        "CPython custom-type smoke test failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("SMOKE-CUSTOM-OK"),
        "unexpected stdout: {stdout}"
    );

    let _ = mudu_sys::fs::sync::sync_remove_dir_all(&tmp_pb);
    Ok(())
}

fn unique_temp_dir(prefix: &str) -> Result<std::path::PathBuf, Box<dyn Error>> {
    Ok(mudu_sys::env_var::temp_dir().join(format!(
        "{prefix}_{}",
        mudu_sys::time::system_time_now()
            .duration_since(UNIX_EPOCH)?
            .as_nanos()
    )))
}

fn assert_python_syntax(source: &str) -> Result<(), Box<dyn Error>> {
    let mut parser = Parser::new();
    let language = tree_sitter_python::LANGUAGE;
    parser.set_language(&language.into())?;
    let tree = parser.parse(source, None).ok_or("failed to parse Python")?;
    assert!(
        !tree.root_node().has_error(),
        "generated Python syntax should be valid"
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
    resolve.push_str("procedures.gen.wit", source)?;
    Ok(())
}
