//! Integration tests for the `mpk` command-line parser and subcommands.
#![allow(missing_docs)]
#![allow(clippy::panic)] // only for unexpected enum branches in parser tests

use crate::*;
use anyhow::Result;
use mudu::utils::json::to_json_str;
use mudu_contract::procedure::mod_proc_desc::ModProcDesc;
use mudu_sys::fs::sync::sync_write;
use std::collections::HashMap;
use std::ffi::OsString;
use tempfile::TempDir;

fn create_valid_package_files(dir: &TempDir, module_name: &str) -> Result<()> {
    let cfg = r#"{"name":"myapp","lang":"rust","version":"0.1.0","use_async":true}"#;
    sync_write(dir.path().join("package.cfg.json"), cfg)?;

    let mut modules = HashMap::new();
    modules.insert(module_name.to_string(), Vec::new());
    let desc = ModProcDesc::new(modules);
    sync_write(dir.path().join("package.desc.json"), to_json_str(&desc)?)?;

    sync_write(dir.path().join("ddl.sql"), "CREATE TABLE t (id INT);")?;
    sync_write(dir.path().join("initdb.sql"), "INSERT INTO t VALUES (1);")?;
    sync_write(dir.path().join(format!("{}.wasm", module_name)), b"\0asm")?;
    Ok(())
}

#[test]
fn parse_create_success() -> Result<()> {
    let dir = TempDir::new()?;
    create_valid_package_files(&dir, "mod1")?;

    let args: Vec<OsString> = vec![
        "mpk".into(),
        "create".into(),
        "--package-cfg".into(),
        dir.path().join("package.cfg.json").into_os_string(),
        "--package-desc".into(),
        dir.path().join("package.desc.json").into_os_string(),
        "--ddl-sql".into(),
        dir.path().join("ddl.sql").into_os_string(),
        "--initdb-sql".into(),
        dir.path().join("initdb.sql").into_os_string(),
        "--wasm-files".into(),
        dir.path().join("mod1.wasm").into_os_string(),
        "--output".into(),
        dir.path().join("out.mpk").into_os_string(),
    ];
    let cmd = parse_arguments_from(args)?;

    let MPKCommand::Package(pkg) = cmd else {
        panic!("expected Package command");
    };
    assert_eq!(
        pkg.output_path,
        dir.path().join("out.mpk").to_string_lossy().to_string()
    );
    Ok(())
}

#[test]
fn parse_create_uses_default_output_from_app_name() -> Result<()> {
    let dir = TempDir::new()?;
    create_valid_package_files(&dir, "mod1")?;

    let args: Vec<OsString> = vec![
        "mpk".into(),
        "create".into(),
        "--package-cfg".into(),
        dir.path().join("package.cfg.json").into_os_string(),
        "--package-desc".into(),
        dir.path().join("package.desc.json").into_os_string(),
        "--ddl-sql".into(),
        dir.path().join("ddl.sql").into_os_string(),
        "--initdb-sql".into(),
        dir.path().join("initdb.sql").into_os_string(),
        "--wasm-files".into(),
        dir.path().join("mod1.wasm").into_os_string(),
    ];
    let cmd = parse_arguments_from(args)?;

    let MPKCommand::Package(pkg) = cmd else {
        panic!("expected Package command");
    };
    assert_eq!(pkg.output_path, "myapp.mpk");
    Ok(())
}

#[test]
fn parse_create_rejects_missing_required_argument() {
    let result = parse_arguments_from(["mpk", "create", "--package-cfg", "/tmp/cfg.json"]);
    assert!(result.is_err());
}

#[test]
fn parse_create_rejects_validation_failure() -> Result<()> {
    let dir = TempDir::new()?;
    create_valid_package_files(&dir, "mod1")?;

    let args: Vec<OsString> = vec![
        "mpk".into(),
        "create".into(),
        "--package-cfg".into(),
        dir.path().join("package.cfg.json").into_os_string(),
        "--package-desc".into(),
        dir.path().join("package.desc.json").into_os_string(),
        "--ddl-sql".into(),
        dir.path().join("ddl.sql").into_os_string(),
        "--initdb-sql".into(),
        dir.path().join("initdb.sql").into_os_string(),
        "--wasm-files".into(),
        dir.path().join("missing.wasm").into_os_string(),
    ];
    let result = parse_arguments_from(args);
    assert!(result.is_err());
    Ok(())
}

#[test]
fn parse_merge_desc_success() -> Result<()> {
    let dir = TempDir::new()?;
    let input = dir.path().join("input");
    mudu_sys::fs::sync::sync_create_dir_all(&input)?;

    let mut modules = HashMap::new();
    modules.insert("m".to_string(), Vec::new());
    let desc = ModProcDesc::new(modules);
    sync_write(input.join("m.desc.json"), to_json_str(&desc)?)?;

    let output = dir.path().join("merged.desc.json");

    let args: Vec<OsString> = vec![
        "mpk".into(),
        "merge-desc".into(),
        "--input-folder".into(),
        input.clone().into_os_string(),
        "--output-desc-file".into(),
        output.clone().into_os_string(),
    ];
    let cmd = parse_arguments_from(args)?;

    let MPKCommand::MergeDesc(merge) = cmd else {
        panic!("expected MergeDesc command");
    };
    assert_eq!(merge.input_folder, input.to_string_lossy().to_string());
    assert_eq!(merge.output_desc_file, output.to_string_lossy().to_string());
    Ok(())
}

#[test]
fn parse_merge_desc_rejects_missing_argument() {
    let result = parse_arguments_from(["mpk", "merge-desc", "--input-folder", "/tmp/input"]);
    assert!(result.is_err());
}

#[test]
fn parse_create_from_toml_success() -> Result<()> {
    let dir = TempDir::new()?;
    create_valid_package_files(&dir, "mod1")?;

    let toml_content = format!(
        r#"
package_cfg = "{}"
package_desc = "{}"
ddl_sql = "{}"
initdb_sql = "{}"
wasm_files = ["{}"]
output_path = "{}"
"#,
        dir.path().join("package.cfg.json").to_string_lossy(),
        dir.path().join("package.desc.json").to_string_lossy(),
        dir.path().join("ddl.sql").to_string_lossy(),
        dir.path().join("initdb.sql").to_string_lossy(),
        dir.path().join("mod1.wasm").to_string_lossy(),
        dir.path().join("from_toml.mpk").to_string_lossy(),
    );
    let toml_path = dir.path().join("args.toml");
    sync_write(&toml_path, toml_content)?;

    let args: Vec<OsString> = vec![
        "mpk".into(),
        "create-from-toml".into(),
        "--toml".into(),
        toml_path.into_os_string(),
    ];
    let cmd = parse_arguments_from(args)?;

    let MPKCommand::Package(pkg) = cmd else {
        panic!("expected Package command");
    };
    assert!(pkg.output_path.ends_with("from_toml.mpk"));
    Ok(())
}

#[test]
fn parse_rejects_unknown_subcommand() {
    let result = parse_arguments_from(["mpk", "unknown", "--foo", "bar"]);
    assert!(result.is_err());
}

#[test]
fn parse_rejects_unknown_flag() {
    let result = parse_arguments_from(["mpk", "merge-desc", "--unknown"]);
    assert!(result.is_err());
}
