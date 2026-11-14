//! End-to-end tests for the `mpm` binary: it reads `mpm.toml` and runs the
//! named Python build script in the manifest's directory.

#![allow(clippy::unwrap_used)]

use mudu_sys::process::Command;
use std::process::Stdio;

fn temp_project(tag: &str, script_body: &str) -> std::path::PathBuf {
    let dir = mudu_sys::env_var::temp_dir().join(format!("mpm_bin_{tag}_{}", std::process::id()));
    let _ = mudu_sys::fs::sync::sync_remove_dir_all(&dir);
    mudu_sys::fs::sync::sync_create_dir_all(&dir).unwrap();
    mudu_sys::fs::sync::sync_write(dir.join("build.py"), script_body).unwrap();
    mudu_sys::fs::sync::sync_write(
        dir.join("mpm.toml"),
        "[build]\nscript = \"build.py\"\nargs = [\"--fast\"]\n",
    )
    .unwrap();
    dir
}

fn run_mpm(args: &[&str], cwd: &std::path::Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_mpm"))
        .args(args)
        .current_dir(cwd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .unwrap()
}

#[test]
fn build_runs_the_manifest_script_with_args() {
    let dir = temp_project(
        "ok",
        "import sys, pathlib; pathlib.Path('ran.txt').write_text(' '.join(sys.argv[1:]))",
    );
    let manifest = dir.join("mpm.toml");
    let out = run_mpm(
        &["build", "--manifest-path", manifest.to_str().unwrap()],
        &dir,
    );
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    // The script ran in the manifest's directory with the manifest's args.
    assert_eq!(
        mudu_sys::fs::sync::sync_read_to_string(dir.join("ran.txt")).unwrap(),
        "--fast"
    );
    let _ = mudu_sys::fs::sync::sync_remove_dir_all(&dir);
}

#[test]
fn build_fails_for_missing_manifest() {
    let dir = temp_project("missing", "pass");
    let out = run_mpm(&["build", "--manifest-path", "no-such.toml"], &dir);
    assert!(!out.status.success());
    let _ = mudu_sys::fs::sync::sync_remove_dir_all(&dir);
}

#[test]
fn build_fails_for_missing_script() {
    let dir = temp_project("noscript", "pass");
    mudu_sys::fs::sync::sync_remove_file(dir.join("build.py")).unwrap();
    let manifest = dir.join("mpm.toml");
    let out = run_mpm(
        &["build", "--manifest-path", manifest.to_str().unwrap()],
        &dir,
    );
    assert!(!out.status.success());
    let _ = mudu_sys::fs::sync::sync_remove_dir_all(&dir);
}

#[test]
fn build_fails_when_the_script_fails() {
    let dir = temp_project("fail", "import sys; sys.exit(3)");
    let manifest = dir.join("mpm.toml");
    let out = run_mpm(
        &["build", "--manifest-path", manifest.to_str().unwrap()],
        &dir,
    );
    assert!(!out.status.success());
    let _ = mudu_sys::fs::sync::sync_remove_dir_all(&dir);
}
