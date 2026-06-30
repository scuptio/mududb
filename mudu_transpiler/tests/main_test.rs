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
