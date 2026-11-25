//! Verify the real command entry point and its diagnostic exit status.
use anyhow::Result;
use mudu_sys::process::Command;
use tempfile::TempDir;

#[test]
fn cli_creates_python_project_and_rejects_invalid_name() -> Result<()> {
    let dir = TempDir::new()?;
    let output = Command::new(env!("CARGO_BIN_EXE_mpm-crate"))
        .args(["demo", "--lang", "python", "--path"])
        .arg(dir.path())
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("target/py-guest/demo.mpk"), "{stdout}");
    assert!(dir.path().join("demo/readme.md").is_file());
    let output = Command::new(env!("CARGO_BIN_EXE_mpm-crate"))
        .args(["BadName", "--lang", "rust", "--path"])
        .arg(dir.path())
        .output()?;
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("lowercase"));
    assert!(!dir.path().join("BadName").exists());
    Ok(())
}
