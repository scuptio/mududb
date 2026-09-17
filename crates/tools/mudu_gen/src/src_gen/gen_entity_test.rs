//! Unit tests for entity generation from DDL SQL.

#![allow(missing_docs)]
#![allow(clippy::panic)]

use crate::src_gen::gen_entity::gen_rust;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use std::path::Path;

fn path_to_string(path: &Path) -> RS<String> {
    path.to_str()
        .map(|s| s.to_string())
        .ok_or_else(|| mudu_error!(ErrorCode::InvalidUtf8, "path is not valid UTF-8"))
}

fn ddl_path() -> RS<String> {
    path_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/src_gen/ddl_item.sql")
            .as_path(),
    )
}

#[test]
#[cfg_attr(miri, ignore)]
fn gen_rust_creates_output_files() -> RS<()> {
    let output = mudu_sys::env_var::temp_dir().join("mudu_gen_entity_test_output");
    let _ = mudu_sys::fs::sync::sync_remove_dir_all(&output);

    gen_rust(
        vec![ddl_path()?],
        path_to_string(&output)?,
        None,
        "Rust".to_string(),
    )?;

    assert!(mudu_sys::fs::sync::sync_path_exists(&output));
    let entries = mudu_sys::fs::sync::sync_read_dir_entries(&output)?;
    assert!(!entries.is_empty());

    let _ = mudu_sys::fs::sync::sync_remove_dir_all(&output);
    Ok(())
}

#[test]
#[cfg_attr(miri, ignore)]
fn gen_rust_writes_type_desc_when_requested() -> RS<()> {
    let output = mudu_sys::env_var::temp_dir().join("mudu_gen_entity_type_desc_test");
    let ty_desc = mudu_sys::env_var::temp_dir().join("mudu_gen_entity_type_desc_test.json");
    let _ = mudu_sys::fs::sync::sync_remove_dir_all(&output);
    let _ = mudu_sys::fs::sync::sync_remove_file(&ty_desc);

    gen_rust(
        vec![ddl_path()?],
        path_to_string(&output)?,
        Some(path_to_string(&ty_desc)?),
        "Rust".to_string(),
    )?;

    assert!(mudu_sys::fs::sync::sync_path_exists(&ty_desc));

    let _ = mudu_sys::fs::sync::sync_remove_dir_all(&output);
    let _ = mudu_sys::fs::sync::sync_remove_file(&ty_desc);
    Ok(())
}

#[test]
fn gen_rust_rejects_non_directory_output() -> RS<()> {
    let output = mudu_sys::env_var::temp_dir().join("mudu_gen_entity_not_dir.txt");
    mudu_sys::fs::sync::sync_write(&output, b"")?;

    let err = match gen_rust(
        vec![ddl_path()?],
        path_to_string(&output)?,
        None,
        "Rust".to_string(),
    ) {
        Err(e) => e,
        Ok(_) => panic!("expected an error"),
    };

    assert_eq!(err.ec(), ErrorCode::NotADirectory);
    let _ = mudu_sys::fs::sync::sync_remove_file(&output);
    Ok(())
}

#[test]
fn gen_rust_rejects_unknown_language() -> RS<()> {
    let output = mudu_sys::env_var::temp_dir();
    let err = match gen_rust(
        vec![ddl_path()?],
        path_to_string(&output)?,
        None,
        "Java".to_string(),
    ) {
        Err(e) => e,
        Ok(_) => panic!("expected an error"),
    };

    assert_eq!(err.ec(), ErrorCode::Decode);
    Ok(())
}
