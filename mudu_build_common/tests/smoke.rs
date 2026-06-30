//! Smoke tests for `mudu_build_common` build helpers.
#![allow(clippy::unwrap_used)]

use mudu_build_common::{
    collect_universal_files, generate_demo_manifest, generate_sdk_manifest, generate_universal_mod,
    read_workspace_versions, remove_stale_files, repo_root, write_if_changed,
};
use mudu_sys::fs::sync::{
    sync_create_dir_all, sync_read_to_string, sync_remove_dir_all, sync_write,
};
use std::collections::BTreeMap;
use std::path::PathBuf;

fn tmp_dir() -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    mudu_sys::env_var::temp_dir().join(format!(
        "mudu_build_common_test_{}_{}",
        std::process::id(),
        n
    ))
}

fn clean_tmp(path: &std::path::Path) {
    let _ = sync_remove_dir_all(path);
}

#[test]
fn repo_root_returns_parent_of_manifest_dir() {
    let root = repo_root().unwrap();
    assert!(root.is_absolute());
    assert!(root.join("Cargo.toml").exists());
}

#[test]
fn read_workspace_versions_extracts_versions() {
    let manifest = r#"
[workspace]
members = ["mudu_kernel"]

[workspace.dependencies]
serde = "1.0"
tokio = { version = "1.35", features = ["full"] }
rmp-serde = "1.1"
"#;
    let versions = read_workspace_versions(manifest, &["serde", "tokio", "rmp-serde"]).unwrap();
    assert_eq!(versions["serde"], "1.0");
    assert_eq!(versions["tokio"], "1.35");
    assert_eq!(versions["rmp-serde"], "1.1");
}

#[test]
fn read_workspace_versions_errors_on_missing_key() {
    let manifest = "[workspace]\n[workspace.dependencies]\nserde = \"1.0\"\n";
    let err = read_workspace_versions(manifest, &["serde", "missing"]).unwrap_err();
    assert!(err.to_string().contains("missing workspace dep"));
}

#[test]
fn write_if_changed_only_writes_when_content_differs() {
    let base = tmp_dir();
    clean_tmp(&base);
    let path = base.join("write_if_changed.txt");

    write_if_changed(&path, "first").unwrap();
    let first_meta = path.metadata().unwrap();

    write_if_changed(&path, "first").unwrap();
    let second_meta = path.metadata().unwrap();
    assert_eq!(
        first_meta.modified().unwrap(),
        second_meta.modified().unwrap()
    );

    write_if_changed(&path, "second").unwrap();
    let third_meta = path.metadata().unwrap();
    assert_ne!(
        first_meta.modified().unwrap(),
        third_meta.modified().unwrap()
    );
    assert_eq!(sync_read_to_string(&path).unwrap(), "second");
    clean_tmp(&base);
}

#[test]
fn remove_stale_files_keeps_only_wanted_files() {
    let base = tmp_dir();
    clean_tmp(&base);
    let dir = base.join("stale");
    sync_create_dir_all(&dir).unwrap();
    sync_write(dir.join("keep.txt"), "keep").unwrap();
    sync_write(dir.join("remove.txt"), "remove").unwrap();

    remove_stale_files(&dir, &["keep.txt".to_string()], None).unwrap();
    assert!(dir.join("keep.txt").exists());
    assert!(!dir.join("remove.txt").exists());
    clean_tmp(&base);
}

#[test]
fn collect_universal_files_ignores_mod_rs_and_non_rs() {
    let base = tmp_dir();
    clean_tmp(&base);
    let dir = base.join("universal");
    sync_create_dir_all(&dir).unwrap();
    sync_write(dir.join("a.rs"), "").unwrap();
    sync_write(dir.join("mod.rs"), "").unwrap();
    sync_write(dir.join("b.txt"), "").unwrap();

    let files = collect_universal_files(&dir).unwrap();
    assert_eq!(files, vec!["a.rs"]);
    clean_tmp(&base);
}

#[test]
fn generate_universal_mod_declares_modules() {
    let out = generate_universal_mod(&["a.rs".to_string(), "b.rs".to_string()]);
    assert!(out.contains("pub mod a;"));
    assert!(out.contains("pub mod b;"));
}

#[test]
fn generate_sdk_manifest_fills_versions() {
    let mut versions = BTreeMap::new();
    versions.insert("rmp-serde".to_string(), "1.0".to_string());
    versions.insert("serde".to_string(), "1.1".to_string());
    versions.insert("serde_repr".to_string(), "1.2".to_string());
    versions.insert("tokio".to_string(), "1.3".to_string());
    versions.insert("wit-bindgen".to_string(), "1.4".to_string());
    versions.insert("rusqlite".to_string(), "1.5".to_string());

    let manifest = generate_sdk_manifest(&versions);
    assert!(manifest.contains("rmp-serde = \"1.0\""));
    assert!(manifest.contains("version = \"1.1\""));
    assert!(manifest.contains("serde_repr = \"1.2\""));
    assert!(manifest.contains("tokio = { version = \"1.3\""));
    assert!(manifest.contains("wit-bindgen = \"1.4\""));
    assert!(manifest.contains("rusqlite = { version = \"1.5\""));
}

#[test]
fn generate_demo_manifest_uses_tokio_version() {
    let mut versions = BTreeMap::new();
    versions.insert("tokio".to_string(), "9.9".to_string());
    let manifest = generate_demo_manifest(&versions);
    assert!(manifest.contains("tokio = { version = \"9.9\""));
}
