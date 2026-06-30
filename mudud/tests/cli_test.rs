//! Integration tests for the `mudud` binary entry point.

// Miri cannot spawn external processes (posix_spawnattr_init is unsupported).
#![cfg(all(test, not(miri)))]
#![allow(missing_docs)]

use mudu_sys::process::Command;

#[test]
fn binary_prints_help() -> Result<(), Box<dyn std::error::Error>> {
    let output = Command::new(env!("CARGO_BIN_EXE_mudud"))
        .arg("--help")
        .output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "stdout: {stdout}");
    assert!(stdout.contains("MuduDB server"));
    Ok(())
}

#[test]
fn binary_rejects_unknown_flag() -> Result<(), Box<dyn std::error::Error>> {
    let output = Command::new(env!("CARGO_BIN_EXE_mudud"))
        .arg("--unknown-flag")
        .output()?;

    assert!(!output.status.success());
    Ok(())
}

#[test]
fn binary_logs_serve_error() -> Result<(), Box<dyn std::error::Error>> {
    let cfg = mudu_sys::env_var::temp_dir().join("invalid_mudud_cfg.toml");
    mudu_sys::fs::sync::sync_write(&cfg, "not valid toml")?;

    let output = Command::new(env!("CARGO_BIN_EXE_mudud"))
        .arg("--cfg")
        .arg(&cfg)
        .output()?;

    // The binary exits successfully after logging the error.
    assert!(output.status.success());
    let combined = format!(
        "{} {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        combined.contains("mududb serve run error"),
        "output: {combined}"
    );
    Ok(())
}
