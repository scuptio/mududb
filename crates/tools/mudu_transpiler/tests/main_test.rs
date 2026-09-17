//! Integration tests that exercise the `mtp` binary entry point.

// Miri cannot spawn external processes (posix_spawnattr_init is unsupported).
#![cfg(all(test, not(miri)))]
#![allow(missing_docs)]

use mudu_sys::process::Command;
use mudu_utils::this_file;
use std::error::Error;
use std::path::PathBuf;

#[allow(clippy::unwrap_used)]
fn test_data_dir() -> PathBuf {
    PathBuf::from(this_file!())
        .parent()
        .unwrap()
        .join("../src/test_data")
}

fn temp_dir() -> PathBuf {
    mudu_sys::env_var::temp_dir()
}

#[test]
fn binary_runs_rust_transpilation_successfully() -> Result<(), Box<dyn Error>> {
    let dir = test_data_dir();
    let tmp = temp_dir();
    let output = tmp.join("mtp_bin_out.rs");
    let desc = tmp.join("mtp_bin_desc.json");

    let output = Command::new(env!("CARGO_BIN_EXE_mtp"))
        .arg("-i")
        .arg(dir.join("procedure.rs"))
        .arg("-o")
        .arg(&output)
        .arg("-m")
        .arg("test")
        .arg("-p")
        .arg(&desc)
        .arg("-t")
        .arg(dir.join("types.desc.json"))
        .arg("rust")
        .output()?;

    assert!(
        output.status.success(),
        "mtp binary failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

#[test]
fn binary_reports_error_for_invalid_input() -> Result<(), Box<dyn Error>> {
    let tmp = temp_dir();
    let output = tmp.join("missing_parent/mtp_bin_err.rs");

    let output = Command::new(env!("CARGO_BIN_EXE_mtp"))
        .arg("-i")
        .arg("/does/not/exist.rs")
        .arg("-o")
        .arg(&output)
        .arg("rust")
        .output()?;

    assert!(!output.status.success());
    Ok(())
}

#[test]
fn binary_rejects_missing_required_arguments() -> Result<(), Box<dyn Error>> {
    let output = Command::new(env!("CARGO_BIN_EXE_mtp"))
        .arg("-o")
        .arg("/tmp/out.rs")
        .arg("rust")
        .output()?;

    assert!(!output.status.success());
    Ok(())
}

/// One binary round-trip per byte-pipe guest front-end: transpile the
/// `src` source with `mtp <lang>` and assert the three artifacts (adapter,
/// world WIT, package desc) land on disk.
fn binary_runs_frontend(
    lang: &str,
    file_name: &str,
    source: &str,
    module: &str,
    expect: &[&str],
) -> Result<(), Box<dyn Error>> {
    let tmp = temp_dir().join(format!(
        "mtp_bin_{lang}_{}",
        mudu_sys::time::system_time_now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos()
    ));
    mudu_sys::fs::sync::sync_create_dir_all(&tmp)?;
    let input = tmp.join(file_name);
    mudu_sys::fs::sync::sync_write(&input, source)?;
    let stem = tmp.join("procedures_gen");
    let output = match lang {
        "csharp" => stem.with_extension("cs"),
        "go" => stem.with_extension("go"),
        _ => stem.with_extension("c"),
    };
    let wit = output.with_extension("wit");
    let desc = tmp.join("package.desc.json");

    let output_cmd = Command::new(env!("CARGO_BIN_EXE_mtp"))
        .arg("-i")
        .arg(&input)
        .arg("-o")
        .arg(&output)
        .arg("-m")
        .arg(module)
        .arg("-p")
        .arg(&desc)
        .arg(lang)
        .output()?;

    assert!(
        output_cmd.status.success(),
        "mtp {lang} failed: {}",
        String::from_utf8_lossy(&output_cmd.stderr)
    );
    let wit_text = mudu_sys::fs::sync::sync_read_to_string(&wit)?;
    let desc_text = mudu_sys::fs::sync::sync_read_to_string(&desc)?;
    for fragment in expect {
        assert!(
            wit_text.contains(fragment),
            "mtp {lang} wit should contain `{fragment}`:\n{wit_text}"
        );
    }
    assert!(desc_text.contains("\"module_name\""));
    Ok(())
}

#[test]
fn binary_runs_csharp_transpilation_successfully() -> Result<(), Box<dyn Error>> {
    binary_runs_frontend(
        "csharp",
        "Procedures.cs",
        r#"
internal static class Procedures
{
    // mudu-proc
    public static long CreateItem(MuduOid session, long itemId, string name)
    {
        return itemId;
    }
}
"#,
        "demo_cs",
        &[
            "world demo-cs {",
            "export mp2-create-item: func(param: list<u8>) -> list<u8>;",
        ],
    )?;
    // The `cs` alias resolves to the same front-end.
    let tmp = temp_dir();
    let output = Command::new(env!("CARGO_BIN_EXE_mtp"))
        .arg("-i")
        .arg("/does/not/exist.cs")
        .arg("-o")
        .arg(tmp.join("mtp_bin_cs_alias.cs"))
        .arg("cs")
        .output()?;
    assert!(!output.status.success());
    Ok(())
}

#[test]
fn binary_runs_c_transpilation_successfully() -> Result<(), Box<dyn Error>> {
    binary_runs_frontend(
        "c",
        "procedures.c",
        r#"
#include "mudu_sys.h"

// mudu-proc (item_id: i64, name: string) -> i64
int create_item(const mudu_proc_param *param, mudu_datum *result, mudu_error *err) {
    *result = mudu_i64(param->params[0].i64);
    return 0;
}
"#,
        "demo_c",
        &[
            "world demo-c {",
            "export mp2-create-item: func(param: list<u8>) -> list<u8>;",
        ],
    )
}

#[test]
fn binary_runs_go_transpilation_successfully() -> Result<(), Box<dyn Error>> {
    binary_runs_frontend(
        "go",
        "procedures.go",
        r#"
package main

// mudu-proc
func createItem(session muduOid, itemID int64, name string) (int64, error) {
	return itemID, nil
}
"#,
        "demo_go",
        &[
            "world demo-go {",
            "    include wasi:cli/imports@0.2.0;\n",
            "export mp2-create-item: func(param: list<u8>) -> list<u8>;",
        ],
    )?;
    // The `golang` alias resolves to the same front-end.
    let tmp = temp_dir();
    let output = Command::new(env!("CARGO_BIN_EXE_mtp"))
        .arg("-i")
        .arg("/does/not/exist.go")
        .arg("-o")
        .arg(tmp.join("mtp_bin_go_alias.go"))
        .arg("golang")
        .output()?;
    assert!(!output.status.success());
    Ok(())
}
