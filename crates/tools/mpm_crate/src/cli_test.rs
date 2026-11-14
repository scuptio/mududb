//! Tests for the `mpm-crate` command-line parser.
#![allow(missing_docs)]

use crate::*;
use anyhow::Result;

#[test]
fn parse_minimal_rust() -> Result<()> {
    let cli = Cli::try_parse_from(["mpm-crate", "demo", "--lang", "rust"])?;
    assert_eq!(cli.name, "demo");
    assert_eq!(cli.lang, Lang::Rust);
    assert!(cli.path.is_none());
    assert!(cli.sdk_path.is_none());
    Ok(())
}

#[test]
fn parse_all_languages() -> Result<()> {
    for (arg, lang) in [
        ("rust", Lang::Rust),
        ("rs", Lang::Rust),
        ("assemblyscript", Lang::AssemblyScript),
        ("as", Lang::AssemblyScript),
        ("csharp", Lang::Csharp),
        ("cs", Lang::Csharp),
        ("python", Lang::Python),
        ("py", Lang::Python),
        ("c", Lang::C),
        ("cc", Lang::C),
        ("cpp", Lang::C),
        ("go", Lang::Go),
        ("golang", Lang::Go),
    ] {
        let cli = Cli::try_parse_from(["mpm-crate", "demo", "--lang", arg])?;
        assert_eq!(cli.lang, lang);
    }
    Ok(())
}

#[test]
fn parse_rejects_unknown_language() {
    let cli = Cli::try_parse_from(["mpm-crate", "demo", "--lang", "java"]);
    assert!(cli.is_err());
}

#[test]
fn parse_rejects_missing_language() {
    let cli = Cli::try_parse_from(["mpm-crate", "demo"]);
    assert!(cli.is_err());
}

#[test]
fn parse_rejects_missing_name() {
    let cli = Cli::try_parse_from(["mpm-crate", "--lang", "rust"]);
    assert!(cli.is_err());
}

#[test]
fn parse_full_arguments() -> Result<()> {
    let cli = Cli::try_parse_from([
        "mpm-crate",
        "my-app",
        "--lang",
        "rust",
        "--path",
        "/tmp/scaffold",
        "--sdk-path",
        "/opt/mududb",
    ])?;
    assert_eq!(cli.name, "my-app");
    assert_eq!(
        cli.path.map(|p| p.to_string_lossy().to_string()),
        Some("/tmp/scaffold".to_string())
    );
    assert_eq!(
        cli.sdk_path.map(|p| p.to_string_lossy().to_string()),
        Some("/opt/mududb".to_string())
    );
    Ok(())
}

fn temp_parent(tag: &str) -> Result<std::path::PathBuf> {
    let dir =
        mudu_sys::env_var::temp_dir().join(format!("mpm_crate_run_{tag}_{}", std::process::id()));
    let _ = mudu_sys::fs::sync::sync_remove_dir_all(&dir);
    mudu_sys::fs::sync::sync_create_dir_all(&dir)?;
    Ok(dir)
}

#[test]
fn run_scaffolds_projects_for_each_language_family() -> Result<()> {
    // One scaffold per print_report arm: Python and Go print custom package
    // paths; everything else prints the wasm32-wasip2 path.
    for lang in [Lang::Rust, Lang::Python, Lang::Go] {
        let parent = temp_parent(lang.to_string().as_str())?;
        let cli = Cli {
            name: format!("demo-{}", lang.to_string().to_lowercase()),
            lang,
            path: Some(parent.clone()),
            sdk_path: None,
        };
        run(cli)?;
        let report_dir = parent.join(format!("demo-{}", lang.to_string().to_lowercase()));
        assert!(
            mudu_sys::fs::sync::sync_path_exists(&report_dir),
            "missing {}",
            report_dir.display()
        );
        let _ = mudu_sys::fs::sync::sync_remove_dir_all(&parent);
    }
    Ok(())
}

#[test]
fn run_rejects_invalid_name() -> Result<()> {
    let parent = temp_parent("invalid")?;
    let cli = Cli {
        name: "9bad".to_string(),
        lang: Lang::Rust,
        path: Some(parent.clone()),
        sdk_path: None,
    };
    assert!(run(cli).is_err());
    let _ = mudu_sys::fs::sync::sync_remove_dir_all(&parent);
    Ok(())
}
