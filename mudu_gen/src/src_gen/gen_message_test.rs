//! Unit tests for message generation from WIT definitions.

#![allow(missing_docs)]
#![allow(clippy::panic)]

use crate::src_gen::gen_message::gen_message;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use std::path::Path;
use std::path::PathBuf;

fn contract_wit_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/src_gen/contract.wit")
}

fn output_parent(output: &Path) -> RS<&Path> {
    output.parent().ok_or_else(|| {
        mudu_error!(
            ErrorCode::InvalidArgument,
            "output path does not have a parent directory"
        )
    })
}

#[test]
#[cfg_attr(miri, ignore)]
fn gen_message_from_file_writes_rust_output() -> RS<()> {
    let output = mudu_sys::env_var::temp_dir().join("mudu_gen_message_file_test.rs");
    let _ = mudu_sys::fs::sync::sync_remove_file(&output);

    gen_message(contract_wit_path(), &output, "Rust".to_string(), None)?;

    assert!(mudu_sys::fs::sync::sync_path_exists(&output));
    let source = mudu_sys::fs::sync::sync_read_to_string(&output)?;
    assert!(source.contains("pub"));
    let _ = mudu_sys::fs::sync::sync_remove_file(&output);
    Ok(())
}

#[test]
#[cfg_attr(miri, ignore)]
fn gen_message_from_directory_writes_named_output() -> RS<()> {
    let input_dir = mudu_sys::env_var::temp_dir().join("mudu_gen_message_dir_test_in");
    let output_dir = mudu_sys::env_var::temp_dir().join("mudu_gen_message_dir_test_out");
    let _ = mudu_sys::fs::sync::sync_remove_dir_all(&input_dir);
    let _ = mudu_sys::fs::sync::sync_remove_dir_all(&output_dir);
    mudu_sys::fs::sync::sync_create_dir_all(&input_dir)?;
    mudu_sys::fs::sync::sync_copy(contract_wit_path(), input_dir.join("contract.wit"))?;

    gen_message(&input_dir, &output_dir, "Rust".to_string(), None)?;

    let entries = mudu_sys::fs::sync::sync_read_dir_entries(&output_dir)?;
    assert!(!entries.is_empty());

    let _ = mudu_sys::fs::sync::sync_remove_dir_all(&input_dir);
    let _ = mudu_sys::fs::sync::sync_remove_dir_all(&output_dir);
    Ok(())
}

#[test]
#[cfg_attr(miri, ignore)]
fn gen_message_generates_csharp_output() -> RS<()> {
    let output = mudu_sys::env_var::temp_dir().join("mudu_gen_message_cs_test.cs");
    let _ = mudu_sys::fs::sync::sync_remove_file(&output);

    gen_message(
        contract_wit_path(),
        &output,
        "CSharp".to_string(),
        Some("MuduDb".to_string()),
    )?;

    let source = mudu_sys::fs::sync::sync_read_to_string(&output)?;
    assert!(source.contains("namespace MuduDb"));
    let _ = mudu_sys::fs::sync::sync_remove_file(&output);
    Ok(())
}

#[test]
#[cfg_attr(miri, ignore)]
fn gen_message_generates_assemblyscript_output() -> RS<()> {
    let output = mudu_sys::env_var::temp_dir().join("mudu_gen_message_as_test.ts");
    let _ = mudu_sys::fs::sync::sync_remove_file(&output);

    gen_message(
        contract_wit_path(),
        &output,
        "AssemblyScript".to_string(),
        None,
    )?;

    let source = mudu_sys::fs::sync::sync_read_to_string(&output)?;
    assert!(source.contains("export class"));
    let _ = mudu_sys::fs::sync::sync_remove_file(&output);
    Ok(())
}

#[test]
#[cfg_attr(miri, ignore)]
fn gen_message_returns_error_when_output_has_no_parent() -> RS<()> {
    let err = match gen_message(
        contract_wit_path(),
        Path::new("/"),
        "Rust".to_string(),
        None,
    ) {
        Err(e) => e,
        Ok(_) => panic!("expected an error"),
    };

    assert_eq!(err.ec(), ErrorCode::InvalidArgument);
    Ok(())
}

#[test]
fn gen_message_rejects_unknown_language() -> RS<()> {
    let output = mudu_sys::env_var::temp_dir().join("mudu_gen_message_unknown.txt");
    let err = match gen_message(contract_wit_path(), &output, "Java".to_string(), None) {
        Err(e) => e,
        Ok(_) => panic!("expected an error"),
    };

    assert_eq!(err.ec(), ErrorCode::InvalidArgument);
    Ok(())
}

#[test]
#[cfg_attr(miri, ignore)]
fn gen_message_from_directory_generates_csharp_output() -> RS<()> {
    let input_dir = mudu_sys::env_var::temp_dir().join("mudu_gen_message_dir_cs_in");
    let output_dir = mudu_sys::env_var::temp_dir().join("mudu_gen_message_dir_cs_out");
    let _ = mudu_sys::fs::sync::sync_remove_dir_all(&input_dir);
    let _ = mudu_sys::fs::sync::sync_remove_dir_all(&output_dir);
    mudu_sys::fs::sync::sync_create_dir_all(&input_dir)?;
    mudu_sys::fs::sync::sync_copy(contract_wit_path(), input_dir.join("contract.wit"))?;

    gen_message(&input_dir, &output_dir, "CSharp".to_string(), None)?;

    let entries = mudu_sys::fs::sync::sync_read_dir_entries(&output_dir)?;
    assert!(!entries.is_empty());
    let source = mudu_sys::fs::sync::sync_read_to_string(entries[0].path())?;
    assert!(source.contains("class"));

    let _ = mudu_sys::fs::sync::sync_remove_dir_all(&input_dir);
    let _ = mudu_sys::fs::sync::sync_remove_dir_all(&output_dir);
    Ok(())
}

#[test]
#[cfg_attr(miri, ignore)]
fn gen_message_skips_non_wit_files_in_directory() -> RS<()> {
    let input_dir = mudu_sys::env_var::temp_dir().join("mudu_gen_message_dir_skip_in");
    let output_dir = mudu_sys::env_var::temp_dir().join("mudu_gen_message_dir_skip_out");
    let _ = mudu_sys::fs::sync::sync_remove_dir_all(&input_dir);
    let _ = mudu_sys::fs::sync::sync_remove_dir_all(&output_dir);
    mudu_sys::fs::sync::sync_create_dir_all(&input_dir)?;
    mudu_sys::fs::sync::sync_copy(contract_wit_path(), input_dir.join("contract.wit"))?;
    mudu_sys::fs::sync::sync_write(input_dir.join("readme.txt"), b"not a wit file")?;

    gen_message(&input_dir, &output_dir, "Rust".to_string(), None)?;

    let entries = mudu_sys::fs::sync::sync_read_dir_entries(&output_dir)?;
    assert_eq!(entries.len(), 2);

    let _ = mudu_sys::fs::sync::sync_remove_dir_all(&input_dir);
    let _ = mudu_sys::fs::sync::sync_remove_dir_all(&output_dir);
    Ok(())
}

#[test]
#[cfg_attr(miri, ignore)]
fn gen_message_creates_parent_directory_for_file_output() -> RS<()> {
    let output = mudu_sys::env_var::temp_dir()
        .join("mudu_gen_message_nested_parent")
        .join("output.rs");
    let parent = output_parent(&output)?;
    let _ = mudu_sys::fs::sync::sync_remove_dir_all(parent);

    gen_message(contract_wit_path(), &output, "Rust".to_string(), None)?;

    assert!(mudu_sys::fs::sync::sync_path_exists(&output));
    let _ = mudu_sys::fs::sync::sync_remove_dir_all(parent);
    Ok(())
}
#[test]
#[cfg_attr(miri, ignore)]
fn gen_message_matches_golden_fixtures() -> RS<()> {
    let fixture_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/src_gen/fixtures");
    let wit_path = fixture_dir.join("simple.wit");

    let expected_rust = mudu_sys::fs::sync::sync_read_to_string(fixture_dir.join("simple.rs"))?;
    let expected_cs = mudu_sys::fs::sync::sync_read_to_string(fixture_dir.join("simple.cs"))?;

    let rust_output = mudu_sys::env_var::temp_dir().join("mudu_gen_golden_rust.rs");
    let cs_output = mudu_sys::env_var::temp_dir().join("mudu_gen_golden_cs.cs");
    let _ = mudu_sys::fs::sync::sync_remove_file(&rust_output);
    let _ = mudu_sys::fs::sync::sync_remove_file(&cs_output);

    gen_message(&wit_path, &rust_output, "Rust".to_string(), None)?;
    gen_message(
        &wit_path,
        &cs_output,
        "CSharp".to_string(),
        Some("MuduDb".to_string()),
    )?;

    let rust_source = mudu_sys::fs::sync::sync_read_to_string(&rust_output)?;
    let cs_source = mudu_sys::fs::sync::sync_read_to_string(&cs_output)?;
    assert_eq!(rust_source, expected_rust);
    assert_eq!(cs_source, expected_cs);

    let _ = mudu_sys::fs::sync::sync_remove_file(&rust_output);
    let _ = mudu_sys::fs::sync::sync_remove_file(&cs_output);
    Ok(())
}
