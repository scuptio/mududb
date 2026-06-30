#![allow(missing_docs)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::disallowed_methods)]

#[cfg(test)]
mod tests {
    use crate::{
        collect_universal_files, copy_file_if_changed, generate_demo_manifest,
        generate_sdk_manifest, generate_universal_mod, read_workspace_versions, remove_stale_files,
        repo_root, rerun_if_changed, ts_const_generate, write_if_changed,
    };
    use md5::{Digest, Md5};
    use std::collections::BTreeMap;
    use std::fs;
    use std::path::Path;

    fn sample_versions() -> BTreeMap<String, String> {
        let mut versions = BTreeMap::new();
        versions.insert("rmp-serde".to_string(), "1.2.3".to_string());
        versions.insert("serde".to_string(), "1.3.0".to_string());
        versions.insert("serde_repr".to_string(), "0.2.0".to_string());
        versions.insert("tokio".to_string(), "1.40.0".to_string());
        versions.insert("wit-bindgen".to_string(), "0.50.0".to_string());
        versions.insert("rusqlite".to_string(), "0.40.0".to_string());
        versions
    }

    fn md5_of_string(content: &str) -> crate::Result<String> {
        let mut hasher = Md5::new();
        hasher.update(content.as_bytes());
        let hash = hasher.finalize();
        let mut buf = [0u8; 256];
        Ok(base16ct::lower::encode_str(&hash, &mut buf).map(|s| s.to_string())?)
    }

    #[test]
    fn repo_root_returns_manifest_parent() -> crate::Result<()> {
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR")?;
        let expected = Path::new(&manifest_dir)
            .parent()
            .expect("manifest dir has a parent")
            .to_path_buf();
        assert_eq!(repo_root()?, expected);
        Ok(())
    }

    #[test]
    fn rerun_if_changed_runs_without_panic() {
        // The function only emits a `cargo:rerun-if-changed=...` directive.
        // We exercise the public surface by ensuring it does not panic.
        rerun_if_changed(Path::new("some/path/to/file.rs"));
    }

    #[test]
    fn read_workspace_versions_extracts_string_and_table_versions() -> crate::Result<()> {
        let manifest = r#"
[workspace]
members = ["a"]

[workspace.dependencies]
serde = "1.0"
tokio = { version = "2.0", features = ["full"] }
"#;
        let versions = read_workspace_versions(manifest, &["serde", "tokio"])?;
        assert_eq!(versions.get("serde"), Some(&"1.0".to_string()));
        assert_eq!(versions.get("tokio"), Some(&"2.0".to_string()));
        Ok(())
    }

    #[test]
    fn read_workspace_versions_errors_on_missing_dependency() {
        let manifest = r#"
[workspace]
[workspace.dependencies]
serde = "1.0"
"#;
        assert!(read_workspace_versions(manifest, &["serde", "missing"]).is_err());
    }

    #[test]
    fn read_workspace_versions_errors_on_table_without_version() {
        let manifest = r#"
[workspace]
[workspace.dependencies]
serde = { path = "../serde" }
"#;
        assert!(read_workspace_versions(manifest, &["serde"]).is_err());
    }

    #[test]
    fn read_workspace_versions_errors_on_invalid_toml() {
        assert!(read_workspace_versions("not valid toml @ all", &["serde"]).is_err());
    }

    #[test]
    fn write_if_changed_writes_when_target_missing() -> crate::Result<()> {
        let dir = tempfile::tempdir()?;
        let target = dir.path().join("out.txt");
        write_if_changed(&target, "hello")?;
        assert_eq!(fs::read_to_string(&target)?, "hello");
        Ok(())
    }

    #[test]
    fn write_if_changed_overwrites_when_content_differs() -> crate::Result<()> {
        let dir = tempfile::tempdir()?;
        let target = dir.path().join("out.txt");
        fs::write(&target, "old")?;
        write_if_changed(&target, "new")?;
        assert_eq!(fs::read_to_string(&target)?, "new");
        Ok(())
    }

    #[test]
    fn write_if_changed_keeps_content_when_unchanged() -> crate::Result<()> {
        let dir = tempfile::tempdir()?;
        let target = dir.path().join("out.txt");
        fs::write(&target, "same")?;
        write_if_changed(&target, "same")?;
        assert_eq!(fs::read_to_string(&target)?, "same");
        Ok(())
    }

    #[test]
    fn copy_file_if_changed_copies_source_content() -> crate::Result<()> {
        let dir = tempfile::tempdir()?;
        let source = dir.path().join("source.txt");
        let target = dir.path().join("target.txt");
        fs::write(&source, "copy me")?;
        copy_file_if_changed(&source, &target)?;
        assert_eq!(fs::read_to_string(&target)?, "copy me");
        Ok(())
    }

    #[test]
    fn remove_stale_files_keeps_requested_and_removes_others() -> crate::Result<()> {
        let dir = tempfile::tempdir()?;
        let keep_explicit = dir.path().join("keep_explicit.txt");
        let generated = dir.path().join("generated.rs");
        let stale = dir.path().join("stale.rs");
        fs::write(&keep_explicit, "")?;
        fs::write(&generated, "")?;
        fs::write(&stale, "")?;

        remove_stale_files(
            dir.path(),
            &["keep_explicit.txt".to_string()],
            Some("generated.rs"),
        )?;

        assert!(keep_explicit.exists());
        assert!(generated.exists());
        assert!(!stale.exists());
        Ok(())
    }

    #[test]
    fn remove_stale_files_skips_subdirectories() -> crate::Result<()> {
        let dir = tempfile::tempdir()?;
        let subdir = dir.path().join("subdir");
        let file = dir.path().join("file.rs");
        fs::create_dir(&subdir)?;
        fs::write(&file, "")?;

        remove_stale_files(dir.path(), &[], None)?;

        assert!(subdir.exists());
        assert!(!file.exists());
        Ok(())
    }

    #[test]
    fn collect_universal_files_sorts_and_filters() -> crate::Result<()> {
        let dir = tempfile::tempdir()?;
        fs::write(dir.path().join("z.rs"), "")?;
        fs::write(dir.path().join("a.rs"), "")?;
        fs::write(dir.path().join("mod.rs"), "")?;
        fs::write(dir.path().join("readme.txt"), "")?;
        fs::create_dir(dir.path().join("ignored_dir"))?;

        let files = collect_universal_files(dir.path())?;
        assert_eq!(files, vec!["a.rs".to_string(), "z.rs".to_string()]);
        Ok(())
    }

    #[test]
    fn generate_universal_mod_includes_sorted_declarations() {
        let output = generate_universal_mod(&["beta.rs".to_string(), "alpha.rs".to_string()]);
        assert!(output.contains("// Generated by mudu_api_sync. Do not edit manually."));
        assert!(output.contains("pub mod beta;"));
        assert!(output.contains("pub mod alpha;"));
    }

    #[test]
    fn generate_sdk_manifest_substitutes_versions() {
        let manifest = generate_sdk_manifest(&sample_versions());
        assert!(manifest.contains("name = \"mudu_api_rust\""));
        assert!(manifest.contains("rmp-serde = \"1.2.3\""));
        assert!(manifest.contains("serde = { version = \"1.3.0\", features = [\"default\", \"derive\", \"serde_derive\"] }"));
        assert!(manifest.contains("tokio = { version = \"1.40.0\", features = [\"full\"] }"));
        assert!(manifest.contains(
            "rusqlite = { version = \"0.40.0\", features = [\"bundled\"], optional = true }"
        ));
    }

    #[test]
    fn generate_demo_manifest_substitutes_tokio_version() {
        let versions = sample_versions();
        let manifest = generate_demo_manifest(&versions);
        assert!(manifest.contains("name = \"mudu_api_rust_demo\""));
        assert!(manifest.contains("tokio = { version = \"1.40.0\", features = [\"full\"] }"));
    }

    // Miri cannot call the tree-sitter C grammar functions.
    #[cfg_attr(miri, ignore)]
    #[test]
    fn ts_const_generate_errors_on_missing_grammar() -> crate::Result<()> {
        let dir = tempfile::tempdir()?;
        let output = dir.path().join("out");
        let missing_grammar = dir.path().join("missing.json");
        let language = tree_sitter_sql::LANGUAGE.into();
        assert!(ts_const_generate(&output, &missing_grammar, None::<&Path>, language).is_err());
        Ok(())
    }

    // Miri cannot call the tree-sitter C grammar functions.
    #[cfg_attr(miri, ignore)]
    #[test]
    fn ts_const_generate_errors_on_invalid_json() -> crate::Result<()> {
        let dir = tempfile::tempdir()?;
        let grammar = dir.path().join("grammar.json");
        fs::write(&grammar, "not json")?;
        let output = dir.path().join("out");
        let language = tree_sitter_sql::LANGUAGE.into();
        assert!(ts_const_generate(&output, &grammar, None::<&Path>, language).is_err());
        Ok(())
    }

    fn sql_grammar_path() -> crate::Result<std::path::PathBuf> {
        let root = repo_root()?;
        Ok(root
            .join("tree-sitter-sql")
            .join("src")
            .join("grammar.json"))
    }

    // Miri cannot call the tree-sitter C grammar functions.
    #[cfg_attr(miri, ignore)]
    #[test]
    fn ts_const_generate_writes_files_when_no_md5() -> crate::Result<()> {
        let dir = tempfile::tempdir()?;
        let grammar = sql_grammar_path()?;
        let output = dir.path().join("generated");
        let language = tree_sitter_sql::LANGUAGE.into();
        ts_const_generate(&output, &grammar, None::<&Path>, language)?;

        assert!(output.join("ts_field_name.rs").exists());
        assert!(output.join("ts_field_id.rs").exists());
        assert!(output.join("ts_kind_id.rs").exists());
        assert!(output.join("ts_kind_name.rs").exists());
        assert!(output.join("ts_seq_index.rs").exists());
        Ok(())
    }

    // Miri cannot call the tree-sitter C grammar functions.
    #[cfg_attr(miri, ignore)]
    #[test]
    fn ts_const_generate_skips_when_md5_unchanged() -> crate::Result<()> {
        let dir = tempfile::tempdir()?;
        let grammar = sql_grammar_path()?;
        let grammar_content = fs::read_to_string(&grammar)?;
        let md5_value = md5_of_string(&grammar_content)?;

        let output = dir.path().join("generated");
        let md5_path = dir.path().join("grammar.md5");
        fs::write(&md5_path, &md5_value)?;

        let language = tree_sitter_sql::LANGUAGE.into();
        ts_const_generate(&output, &grammar, Some(&md5_path), language)?;

        assert!(!output.join("ts_kind_id.rs").exists());
        assert_eq!(fs::read_to_string(&md5_path)?, md5_value);
        Ok(())
    }

    // Miri cannot call the tree-sitter C grammar functions.
    #[cfg_attr(miri, ignore)]
    #[test]
    fn ts_const_generate_writes_md5_when_changed() -> crate::Result<()> {
        let dir = tempfile::tempdir()?;
        let grammar = sql_grammar_path()?;
        let grammar_content = fs::read_to_string(&grammar)?;
        let expected_md5 = md5_of_string(&grammar_content)?;

        let output = dir.path().join("generated");
        let md5_path = dir.path().join("grammar.md5");
        fs::write(&md5_path, "00000000000000000000000000000000")?;

        let language = tree_sitter_sql::LANGUAGE.into();
        ts_const_generate(&output, &grammar, Some(&md5_path), language)?;

        assert!(output.join("ts_kind_id.rs").exists());
        assert_eq!(fs::read_to_string(&md5_path)?, expected_md5);
        Ok(())
    }

    #[test]
    fn read_workspace_versions_errors_on_non_string_non_table_version() {
        let manifest = r#"
[workspace]
[workspace.dependencies]
serde = 1
"#;
        assert!(read_workspace_versions(manifest, &["serde"]).is_err());
    }

    #[test]
    fn write_if_changed_creates_nested_parent_directories() -> crate::Result<()> {
        let dir = tempfile::tempdir()?;
        let target = dir.path().join("a").join("b").join("out.txt");
        write_if_changed(&target, "nested")?;
        assert_eq!(fs::read_to_string(&target)?, "nested");
        Ok(())
    }

    // Miri cannot call the tree-sitter C grammar functions.
    #[cfg_attr(miri, ignore)]
    #[test]
    fn ts_const_generate_creates_output_directory() -> crate::Result<()> {
        let dir = tempfile::tempdir()?;
        let grammar = sql_grammar_path()?;
        let output = dir.path().join("does").join("not").join("exist");
        let language = tree_sitter_sql::LANGUAGE.into();
        ts_const_generate(&output, &grammar, None::<&Path>, language)?;
        assert!(output.join("ts_kind_id.rs").exists());
        Ok(())
    }

    // Miri cannot call the tree-sitter C grammar functions.
    #[cfg_attr(miri, ignore)]
    #[test]
    fn ts_const_generate_creates_md5_parent_directory() -> crate::Result<()> {
        let dir = tempfile::tempdir()?;
        let grammar = sql_grammar_path()?;
        let output = dir.path().join("generated");
        let md5_path = dir.path().join("nested").join("grammar.md5");
        let language = tree_sitter_sql::LANGUAGE.into();
        ts_const_generate(&output, &grammar, Some(&md5_path), language)?;
        assert!(md5_path.exists());
        Ok(())
    }

    // Miri cannot call the tree-sitter C grammar functions.
    #[cfg_attr(miri, ignore)]
    #[test]
    fn ts_const_generate_with_custom_grammar_exercises_branches() -> crate::Result<()> {
        let dir = tempfile::tempdir()?;
        let grammar = dir.path().join("grammar.json");
        fs::write(
            &grammar,
            r#"{
                "name": "sql",
                "rules": {
                    "rule_seq": {
                        "type": "SEQ",
                        "members": [
                            {"type": "SYMBOL", "name": "x"},
                            {"type": "SYMBOL", "sql": "named_member"},
                            {"type": "SYMBOL", "name": "x"}
                        ]
                    },
                    "rule_alias": {
                        "type": "ALIAS",
                        "content": {"type": "BLANK"},
                        "value": "alias_name"
                    },
                    "rule_alias_special": {
                        "type": "ALIAS",
                        "content": {"type": "BLANK"},
                        "value": "alias-name"
                    },
                    "rule_string": {
                        "type": "STRING",
                        "value": "abc",
                        "sql": "string_literal"
                    }
                }
            }"#,
        )?;
        let output = dir.path().join("generated");
        let language = tree_sitter_sql::LANGUAGE.into();
        ts_const_generate(&output, &grammar, None::<&Path>, language)?;
        assert!(output.join("ts_kind_id.rs").exists());
        let kind_id = fs::read_to_string(output.join("ts_kind_id.rs"))?;
        assert!(kind_id.contains("RULE_SEQ"));
        assert!(kind_id.contains("RULE_ALIAS"));
        assert!(kind_id.contains("ALIAS_NAME"));
        assert!(kind_id.contains("RULE_STRING"));

        let seq_index = fs::read_to_string(output.join("ts_seq_index.rs"))?;
        assert!(seq_index.contains("RULE_SEQ_SEQ_NAMED_MEMBER"));
        Ok(())
    }

    // Miri cannot call the tree-sitter C grammar functions.
    #[cfg_attr(miri, ignore)]
    #[test]
    fn ts_const_generate_errors_when_seq_member_lacks_type() -> crate::Result<()> {
        let dir = tempfile::tempdir()?;
        let grammar = dir.path().join("grammar.json");
        fs::write(
            &grammar,
            r#"{
                "name": "sql",
                "rules": {
                    "bad": {
                        "type": "SEQ",
                        "members": [{"content": {"type": "BLANK"}}]
                    }
                }
            }"#,
        )?;
        let output = dir.path().join("generated");
        let language = tree_sitter_sql::LANGUAGE.into();
        assert!(ts_const_generate(&output, &grammar, None::<&Path>, language).is_err());
        Ok(())
    }
}
