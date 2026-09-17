//! Integration tests for the `mpm-build` binary.

// Miri cannot spawn external processes (posix_spawnattr_init is unsupported).
#![cfg(all(test, not(miri)))]
#![allow(missing_docs)]
#![allow(clippy::panic)] // only for assertion failures in integration tests

use anyhow::Result;
use mudu::utils::json::to_json_str;
use mudu_contract::procedure::mod_proc_desc::ModProcDesc;
use mudu_sys::fs::sync::sync_write;
use mudu_sys::process::Command;
use std::collections::HashMap;
use tempfile::TempDir;

fn create_valid_package_files(dir: &TempDir, module_name: &str) -> Result<()> {
    let cfg = r#"{"name":"binapp","lang":"rust","version":"0.1.0","use_async":true}"#;
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
fn binary_creates_package_successfully() -> Result<()> {
    let dir = TempDir::new()?;
    create_valid_package_files(&dir, "mod1")?;
    let output = dir.path().join("binapp.mpk");

    let out = Command::new(env!("CARGO_BIN_EXE_mpm-build"))
        .arg("create")
        .arg("--package-cfg")
        .arg(dir.path().join("package.cfg.json"))
        .arg("--package-desc")
        .arg(dir.path().join("package.desc.json"))
        .arg("--ddl-sql")
        .arg(dir.path().join("ddl.sql"))
        .arg("--initdb-sql")
        .arg(dir.path().join("initdb.sql"))
        .arg("--wasm-files")
        .arg(dir.path().join("mod1.wasm"))
        .arg("--output")
        .arg(&output)
        .output()?;

    assert!(
        out.status.success(),
        "mpm-build create failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(output.exists());
    Ok(())
}

#[test]
fn binary_merge_desc_successfully() -> Result<()> {
    let dir = TempDir::new()?;
    let input = dir.path().join("input");
    mudu_sys::fs::sync::sync_create_dir_all(&input)?;

    let mut modules = HashMap::new();
    modules.insert("m".to_string(), Vec::new());
    let desc = ModProcDesc::new(modules);
    sync_write(input.join("m.desc.json"), to_json_str(&desc)?)?;

    let output = dir.path().join("merged.desc.json");

    let out = Command::new(env!("CARGO_BIN_EXE_mpm-build"))
        .arg("merge-desc")
        .arg("--input-folder")
        .arg(&input)
        .arg("--output-desc-file")
        .arg(&output)
        .output()?;

    assert!(
        out.status.success(),
        "mpm-build merge-desc failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(output.exists());
    Ok(())
}

#[test]
fn binary_rejects_missing_arguments() -> Result<()> {
    let out = Command::new(env!("CARGO_BIN_EXE_mpm-build"))
        .arg("create")
        .arg("--package-cfg")
        .arg("/tmp/cfg.json")
        .output()?;

    assert!(!out.status.success());
    Ok(())
}
