//! Integration tests for the `mcli` command-line client.

// Miri cannot spawn external processes (posix_spawnattr_init is unsupported).
#![cfg(all(test, not(miri)))]
#![allow(missing_docs)]
#![allow(clippy::unwrap_used)]

use mudu_sys::process::Command;

fn mcli() -> Command {
    Command::new(env!("CARGO_BIN_EXE_mcli"))
}

#[test]
fn help_flag_prints_usage() {
    let output = mcli().arg("--help").output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "stdout: {stdout}");
    assert!(stdout.contains("Usage:"));
    assert!(stdout.contains("mcli"));
}

#[test]
fn version_flag_prints_version() {
    let output = mcli().arg("--version").output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "stdout: {stdout}");
    assert!(stdout.contains(env!("CARGO_PKG_VERSION")));
}

#[test]
fn missing_subcommand_prints_help_and_fails() {
    let output = mcli().output().unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Usage:"));
}

#[test]
fn invalid_subcommand_fails() {
    let output = mcli().arg("not-a-command").output().unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("not-a-command"));
}

#[test]
fn command_subcommand_requires_json_or_json_file() {
    let output = mcli()
        .args(["--addr", "127.0.0.1:9527", "command"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("required"));
}

#[test]
fn conflicting_json_flags_fails() {
    let output = mcli()
        .args([
            "--addr",
            "127.0.0.1:9527",
            "command",
            "--json",
            "{}",
            "--json-file",
            "/dev/null",
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("cannot be used with") || stderr.contains("conflict"));
}
