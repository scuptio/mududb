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
