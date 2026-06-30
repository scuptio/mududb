//! Tests for `SchemaMgr`.

#![allow(missing_docs)]
#![allow(clippy::unwrap_used)]

use crate::resolver::schema_mgr::SchemaMgr;
use std::path::PathBuf;

fn unique_app_name(base: &str) -> String {
    format!("{}-{}", base, mudu_sys::random::uuid_v4())
}

fn target_tmp_dir(prefix: &str) -> PathBuf {
    let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    dir.push("../target/tmp");
    dir.push(format!("{}-{}", prefix, mudu_sys::random::uuid_v4()));
    mudu_sys::fs::sync::create_dir_all(&dir).unwrap();
    dir
}

fn cleanup_mgr(app_name: &String) {
    SchemaMgr::remove_mgr(app_name);
}

#[test]
#[cfg_attr(miri, ignore)]
fn from_sql_text_single_table() {
    let mgr = SchemaMgr::from_sql_text("CREATE TABLE t (id INT PRIMARY KEY, name TEXT);").unwrap();
    let names = mgr.table_names();
    assert!(names.contains(&"t".to_string()));
    assert!(mgr.get(&"t".to_string()).unwrap().is_some());
}

#[test]
#[cfg_attr(miri, ignore)]
fn from_sql_text_multiple_tables() {
    let mgr = SchemaMgr::from_sql_text(
        "CREATE TABLE t1 (id INT PRIMARY KEY); CREATE TABLE t2 (id INT PRIMARY KEY);",
    )
    .unwrap();
    let names = mgr.table_names();
    assert!(names.contains(&"t1".to_string()));
    assert!(names.contains(&"t2".to_string()));
}

#[test]
#[cfg_attr(miri, ignore)]
fn from_sql_text_ignores_non_ddl() {
    let mgr = SchemaMgr::from_sql_text(
        "INSERT INTO dummy VALUES (1); CREATE TABLE t (id INT PRIMARY KEY, name TEXT);",
    )
    .unwrap();
    let names = mgr.table_names();
    assert_eq!(names, vec!["t".to_string()]);
}

#[test]
#[cfg_attr(miri, ignore)]
fn from_sql_text_empty_returns_empty_mgr() {
    let mgr = SchemaMgr::from_sql_text("").unwrap();
    assert!(mgr.table_names().is_empty());
}

#[test]
#[cfg_attr(miri, ignore)]
fn get_missing_table_returns_none() {
    let mgr = SchemaMgr::from_sql_text("CREATE TABLE t (id INT PRIMARY KEY);").unwrap();
    assert!(mgr.get(&"missing".to_string()).unwrap().is_none());
}

#[test]
#[cfg_attr(miri, ignore)]
fn mgr_register_retrieve_and_remove() {
    let app = unique_app_name("schema_mgr_register");
    let mgr = SchemaMgr::from_sql_text("CREATE TABLE t (id INT PRIMARY KEY);").unwrap();
    SchemaMgr::add_mgr(app.clone(), mgr);
    assert!(SchemaMgr::get_mgr(&app).is_some());
    SchemaMgr::remove_mgr(&app);
    assert!(SchemaMgr::get_mgr(&app).is_none());
}

#[test]
#[cfg_attr(miri, ignore)]
fn mgr_remove_missing_is_noop() {
    let app = unique_app_name("schema_mgr_missing");
    cleanup_mgr(&app);
    SchemaMgr::remove_mgr(&app);
    assert!(SchemaMgr::get_mgr(&app).is_none());
}

#[test]
#[cfg_attr(miri, ignore)]
fn load_from_ddl_path_reads_only_sql_files() {
    let dir = target_tmp_dir("ddl_only_sql");
    mudu_sys::fs::sync::write(dir.join("a.sql"), "CREATE TABLE a (id INT PRIMARY KEY);").unwrap();
    mudu_sys::fs::sync::write(dir.join("b.SQL"), "CREATE TABLE b (id INT PRIMARY KEY);").unwrap();
    mudu_sys::fs::sync::write(dir.join("c.txt"), "CREATE TABLE c (id INT PRIMARY KEY);").unwrap();

    let path = dir.to_str().unwrap().to_string();
    let mgr = SchemaMgr::load_from_ddl_path(&path).unwrap();
    let names = mgr.table_names();
    assert!(names.contains(&"a".to_string()));
    assert!(names.contains(&"b".to_string()));
    assert!(!names.contains(&"c".to_string()));
}

#[test]
#[cfg_attr(miri, ignore)]
fn load_from_ddl_path_merges_multiple_files() {
    let dir = target_tmp_dir("ddl_merge");
    mudu_sys::fs::sync::write(dir.join("one.sql"), "CREATE TABLE x (id INT PRIMARY KEY);").unwrap();
    mudu_sys::fs::sync::write(dir.join("two.sql"), "CREATE TABLE y (id INT PRIMARY KEY);").unwrap();

    let path = dir.to_str().unwrap().to_string();
    let mgr = SchemaMgr::load_from_ddl_path(&path).unwrap();
    let names = mgr.table_names();
    assert!(names.contains(&"x".to_string()));
    assert!(names.contains(&"y".to_string()));
}
