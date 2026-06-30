//! Tests for the `mtp` CLI argument parser and the testable `run` entry point.
#![allow(missing_docs)]

use crate::mtp::{Args, CommandType, run};
use clap::Parser;
use mudu_utils::this_file;
use std::error::Error;
use std::path::PathBuf;

#[allow(clippy::unwrap_used)]
fn test_data_dir() -> PathBuf {
    PathBuf::from(this_file!())
        .parent()
        .unwrap()
        .join("test_data")
}

fn temp_dir() -> PathBuf {
    mudu_sys::env_var::temp_dir()
}

#[test]
fn args_parses_rust_subcommand() -> Result<(), Box<dyn Error>> {
    let args = Args::try_parse_from(["mtp", "-i", "in.rs", "-o", "out.rs", "rust"])?;
    assert!(matches!(args.command, CommandType::Rust));
    assert_eq!(args.input, "in.rs");
    assert_eq!(args.output, "out.rs");
    Ok(())
}

#[test]
fn args_parses_assemblyscript_alias() -> Result<(), Box<dyn Error>> {
    let args = Args::try_parse_from(["mtp", "-i", "in.ts", "-o", "out.rs", "as"])?;
    assert!(matches!(args.command, CommandType::AssemblyScript));
    Ok(())
}

#[test]
fn args_parses_optional_flags() -> Result<(), Box<dyn Error>> {
    let args = Args::try_parse_from([
        "mtp",
        "-i",
        "in.rs",
        "-o",
        "out.rs",
        "-m",
        "mod",
        "-s",
        "src",
        "-d",
        "dst",
        "-a",
        "-t",
        "types.json",
        "-p",
        "desc.json",
        "-v",
        "rust",
    ])?;
    assert_eq!(args.module, Some("mod".to_string()));
    assert_eq!(args.src_mod, Some("src".to_string()));
    assert_eq!(args.dst_mod, Some("dst".to_string()));
    assert!(args.enable_async);
    assert_eq!(args.type_desc_file, Some("types.json".to_string()));
    assert_eq!(args.package_desc, Some("desc.json".to_string()));
    assert!(args.verbose);
    Ok(())
}

#[test]
fn args_rejects_missing_input() {
    let result = Args::try_parse_from(["mtp", "-o", "out.rs", "rust"]);
    assert!(result.is_err());
}

#[test]
fn args_rejects_missing_subcommand() {
    let result = Args::try_parse_from(["mtp", "-i", "in.rs", "-o", "out.rs"]);
    assert!(result.is_err());
}

#[test]
fn args_rejects_unknown_flag() {
    let result = Args::try_parse_from(["mtp", "-i", "in.rs", "-o", "out.rs", "--unknown"]);
    assert!(result.is_err());
}

// Miri cannot call the tree-sitter C parser.
#[cfg_attr(miri, ignore)]
#[test]
fn run_succeeds_on_valid_rust_input() -> Result<(), Box<dyn Error>> {
    let dir = test_data_dir();
    let tmp = temp_dir();
    let output = tmp.join("mtp_test_out.rs");
    let desc = tmp.join("mtp_test_desc.json");
    let input = dir.join("procedure.rs");
    let types = dir.join("types.desc.json");

    let args = vec![
        "mtp",
        "-i",
        input.to_str().ok_or("invalid UTF-8 in input path")?,
        "-o",
        output.to_str().ok_or("invalid UTF-8 in output path")?,
        "-m",
        "test",
        "-a",
        "-p",
        desc.to_str().ok_or("invalid UTF-8 in desc path")?,
        "-t",
        types.to_str().ok_or("invalid UTF-8 in types path")?,
        "-v",
        "rust",
    ];

    let result = run(args);
    assert!(result.is_ok(), "expected run to succeed: {:?}", result);
    Ok(())
}

#[cfg_attr(miri, ignore)]
#[test]
fn run_reports_error_when_output_cannot_be_written() -> Result<(), Box<dyn Error>> {
    let dir = test_data_dir();
    let tmp = temp_dir();
    let input = dir.join("procedure.rs");
    // Parent directory does not exist, so writing the output will fail.
    let output = tmp.join("nonexistent_directory/mtp_invalid_out.rs");

    let args = vec![
        "mtp",
        "-i",
        input.to_str().ok_or("invalid UTF-8 in input path")?,
        "-o",
        output.to_str().ok_or("invalid UTF-8 in output path")?,
        "rust",
    ];

    let result = run(args);
    assert!(
        result.is_err(),
        "expected run to fail when output cannot be written"
    );
    let err = result.err().ok_or("expected run to return an error")?;
    assert!(
        err.contains("Rust transpilation failed"),
        "error should mention rust transpilation failure"
    );
    Ok(())
}

#[test]
fn run_reports_error_for_missing_input_file() {
    let args = vec![
        "mtp",
        "-i",
        "/does/not/exist.rs",
        "-o",
        "/tmp/mtp_missing_out.rs",
        "rust",
    ];

    let result = run(args);
    assert!(result.is_err(), "expected run to fail on missing input");
}

#[cfg_attr(miri, ignore)]
#[test]
fn main_inner_succeeds_without_terminating() -> Result<(), Box<dyn Error>> {
    let dir = test_data_dir();
    let tmp = temp_dir();
    let output = tmp.join("mtp_main_inner_out.rs");
    let desc = tmp.join("mtp_main_inner_desc.json");
    let input = dir.join("procedure.rs");
    let types = dir.join("types.desc.json");

    let args = vec![
        "mtp",
        "-i",
        input.to_str().ok_or("invalid UTF-8 in input path")?,
        "-o",
        output.to_str().ok_or("invalid UTF-8 in output path")?,
        "-m",
        "test",
        "-a",
        "-p",
        desc.to_str().ok_or("invalid UTF-8 in desc path")?,
        "-t",
        types.to_str().ok_or("invalid UTF-8 in types path")?,
        "-v",
        "rust",
    ];

    let result = crate::mtp::main_inner(args);
    assert!(result.is_ok());
    Ok(())
}
