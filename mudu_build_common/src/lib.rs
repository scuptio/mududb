#![warn(missing_docs)]
#![warn(clippy::unwrap_used)]
#![deny(clippy::dbg_macro)]
#![warn(clippy::todo)]
#![warn(clippy::unimplemented)]

//! Shared build-script helpers for the MuduDB workspace.

use md5::{Digest, Md5};
use mudu_sys::env_var;
use mudu_sys::fs::sync::{
    SFile, sync_create_dir_all, sync_path_exists, sync_read_dir_entries, sync_read_to_string,
    sync_remove_file, sync_write,
};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::ffi::OsStr;
use std::io::Write;
use std::path::{Path, PathBuf};
use tree_sitter::Language;

#[cfg(test)]
mod build_common_test;

/// Result alias used across this crate for fallible build helpers.
pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn error(s: impl ToString) -> Box<dyn std::error::Error> {
    s.to_string().into()
}

const TS_CONST_COMMENTS: &str = concat!(
    "//\n",
    "// When change grammar.js, re-run ``cargo build`` to generate this file\n",
    "// Caution, do not change this file manually!!!\n",
    "//\n\n"
);

const RULES: &str = "rules";
const TYPE: &str = "type";
const REPEAT: &str = "REPEAT";
const REPEAT1: &str = "REPEAT1";
const SEQ: &str = "SEQ";
const CHOICE: &str = "CHOICE";
const FIELD: &str = "FIELD";
const PREC: &str = "PREC";
const PREC_LEFT: &str = "PREC_LEFT";
const PREC_RIGHT: &str = "PREC_RIGHT";
const ALIAS: &str = "ALIAS";
const MEMBERS: &str = "members";
const CONTENT: &str = "content";
const NAME: &str = "name";
const VALUE: &str = "value";

struct Constant {
    node_name: HashSet<String>,
    field_name: HashSet<String>,
    seq_index: HashMap<String, Vec<usize>>,
}

/// Returns the repository root directory, computed as the parent of
/// `CARGO_MANIFEST_DIR`.
pub fn repo_root() -> Result<PathBuf> {
    let manifest_dir =
        env_var::var("CARGO_MANIFEST_DIR").ok_or_else(|| error("CARGO_MANIFEST_DIR not set"))?;
    let path = PathBuf::from(manifest_dir);
    path.parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| error("CARGO_MANIFEST_DIR has no parent"))
}

/// Wrapper around `println!("cargo:rerun-if-changed=...")`.
pub fn rerun_if_changed(path: &Path) {
    println!("cargo:rerun-if-changed={}", path.display());
}

/// Reads selected workspace dependency versions from the root `Cargo.toml`
/// manifest content.
pub fn read_workspace_versions(
    root_manifest: &str,
    keys: &[&str],
) -> Result<BTreeMap<String, String>> {
    let manifest: toml::Value = toml::from_str(root_manifest)?;
    let deps = manifest["workspace"]["dependencies"]
        .as_table()
        .ok_or_else(|| error("workspace.dependencies table"))?;

    keys.iter()
        .map(|&key| {
            let value = deps
                .get(key)
                .ok_or_else(|| error(format!("missing workspace dep: {key}")))?;
            let version = extract_version(value)
                .ok_or_else(|| error(format!("missing version for {key}")))?;
            Ok((key.to_string(), version))
        })
        .collect()
}

fn extract_version(value: &toml::Value) -> Option<String> {
    match value {
        toml::Value::String(version) => Some(version.clone()),
        toml::Value::Table(table) => table
            .get("version")
            .and_then(|value| value.as_str())
            .map(ToOwned::to_owned),
        _ => None,
    }
}

/// Copies `source` to `target` only if the target content differs.
pub fn copy_file_if_changed(source: &Path, target: &Path) -> Result<()> {
    let content = sync_read_to_string(source)?;
    write_if_changed(target, &content)
}

/// Writes `content` to `path` only if the existing content differs.
pub fn write_if_changed(path: &Path, content: &str) -> Result<()> {
    let needs_write = match sync_read_to_string(path) {
        Ok(existing) => existing != content,
        Err(_) => true,
    };

    if needs_write {
        if let Some(parent) = path.parent() {
            sync_create_dir_all(parent)?;
        }
        sync_write(path, content)?;
    }
    Ok(())
}

/// Removes files in `target_dir` whose names are not present in `keep_files`.
/// If `generated_file` is provided, it is also kept.
pub fn remove_stale_files(
    target_dir: &Path,
    keep_files: &[String],
    generated_file: Option<&str>,
) -> Result<()> {
    let mut keep = keep_files.iter().cloned().collect::<BTreeSet<_>>();
    if let Some(generated_file) = generated_file {
        keep.insert(generated_file.to_string());
    }

    for entry in sync_read_dir_entries(target_dir)? {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let file_name = entry.file_name().to_string_lossy().to_string();
        if keep.contains(&file_name) {
            continue;
        }

        sync_remove_file(&path)?;
    }
    Ok(())
}

/// Collects `.rs` file names in `source_dir`, excluding `mod.rs`.
pub fn collect_universal_files(source_dir: &Path) -> Result<Vec<String>> {
    let mut files = sync_read_dir_entries(source_dir)?
        .into_iter()
        .filter_map(|entry| {
            let path = entry.path();
            let file_name = entry.file_name().to_string_lossy().to_string();

            if !path.is_file()
                || path.extension() != Some(OsStr::new("rs"))
                || file_name == "mod.rs"
            {
                return None;
            }

            Some(file_name)
        })
        .collect::<Vec<_>>();
    files.sort();
    Ok(files)
}

/// Generates a `mod.rs` declaring each file in `universal_files` as a module.
pub fn generate_universal_mod(universal_files: &[String]) -> String {
    let mut content = String::from("// Generated by mudu_api_sync. Do not edit manually.\n\n");
    for file in universal_files {
        let module = file.trim_end_matches(".rs");
        content.push_str(&format!("pub mod {module};\n"));
    }
    content
}

/// Generates the `Cargo.toml` content for the Rust SDK.
pub fn generate_sdk_manifest(versions: &BTreeMap<String, String>) -> String {
    format!(
        concat!(
            "# Generated by mudu_api_sync. Do not edit manually.\n",
            "[package]\n",
            "name = \"mudu_api_rust\"\n",
            "version = \"0.1.0\"\n",
            "edition = \"2024\"\n\n",
            "[workspace]\n",
            "members = [\"demo\"]\n\n",
            "[features]\n",
            "default = []\n",
            "mock-sqlite = [\"dep:rusqlite\"]\n",
            "wasm-async = []\n\n",
            "[dependencies]\n",
            "rmp-serde = \"{rmp_serde}\"\n",
            "serde = {{ version = \"{serde}\", features = [\"default\", \"derive\", \"serde_derive\"] }}\n",
            "serde_repr = \"{serde_repr}\"\n",
            "tokio = {{ version = \"{tokio}\", features = [\"full\"] }}\n",
            "wit-bindgen = \"{wit_bindgen}\"\n",
            "rusqlite = {{ version = \"{rusqlite}\", features = [\"bundled\"], optional = true }}\n",
        ),
        rmp_serde = versions["rmp-serde"],
        serde = versions["serde"],
        serde_repr = versions["serde_repr"],
        tokio = versions["tokio"],
        wit_bindgen = versions["wit-bindgen"],
        rusqlite = versions["rusqlite"],
    )
}

/// Generates the `Cargo.toml` content for the Rust SDK demo.
pub fn generate_demo_manifest(versions: &BTreeMap<String, String>) -> String {
    format!(
        concat!(
            "# Generated by mudu_api_sync. Do not edit manually.\n",
            "[package]\n",
            "name = \"mudu_api_rust_demo\"\n",
            "version = \"0.1.0\"\n",
            "edition = \"2024\"\n\n",
            "[dependencies]\n",
            "mudu_api_rust = {{ path = \"..\", features = [\"mock-sqlite\"] }}\n",
            "tokio = {{ version = \"{tokio}\", features = [\"full\"] }}\n",
        ),
        tokio = versions["tokio"],
    )
}

/// Generates tree-sitter constants from a `grammar.json` file.
///
/// If `md5_path` is `Some`, the grammar MD5 is computed and compared with the
/// previous value; generation is skipped when unchanged and the new MD5 is
/// written after a successful generation. If `md5_path` is `None`, generation
/// always runs.
pub fn ts_const_generate(
    output_path: impl AsRef<Path>,
    grammar_path: impl AsRef<Path>,
    md5_path: Option<impl AsRef<Path>>,
    language: Language,
) -> Result<()> {
    let output_path = output_path.as_ref();
    if !sync_path_exists(output_path) {
        sync_create_dir_all(output_path)?;
    }

    grammar_path
        .as_ref()
        .to_str()
        .ok_or_else(|| error("grammar path is not valid UTF-8"))?;
    let grammar_str = sync_read_to_string(grammar_path)?;

    let new_md5 = md5_path
        .as_ref()
        .map(|_| compute_md5(&grammar_str))
        .transpose()?;
    if let Some(md5_path) = md5_path.as_ref() {
        let previous_md5 = sync_read_to_string(md5_path).ok();
        if previous_md5.as_ref() == new_md5.as_ref() {
            return Ok(());
        }
    }

    let json: Value = serde_json::from_str(&grammar_str)?;

    let mut constant = Constant {
        node_name: HashSet::new(),
        field_name: HashSet::new(),
        seq_index: HashMap::new(),
    };

    let language_name = language
        .name()
        .ok_or_else(|| error("language name is not available"))?
        .to_string();
    visit_rule(&language_name, json, &mut constant)?;
    output_rust_file(&language, output_path, &constant)?;

    if let Some((md5_path, md5)) = md5_path.as_ref().zip(new_md5.as_ref()) {
        let md5_path: &std::path::Path = md5_path.as_ref();
        if let Some(parent) = md5_path.parent() {
            sync_create_dir_all(parent)?;
        }
        sync_write(md5_path, md5)?;
    }
    Ok(())
}

fn compute_md5(s: &str) -> Result<String> {
    let mut hasher = Md5::new();
    hasher.update(s);
    let md5_hash = hasher.finalize();
    let mut buf = [0u8; 256];
    Ok(base16ct::lower::encode_str(&md5_hash, &mut buf).map(|s| s.to_string())?)
}

fn format_name(names: &[String]) -> String {
    let mut name_ret = String::new();
    for (i, name) in names.iter().enumerate() {
        if i == names.len() - 1 {
            name_ret.push_str(name);
            continue;
        }
        let f20char = if name.len() > 20 { &name[0..20] } else { name };
        name_ret.push_str(f20char);
        name_ret.push('_');
    }
    name_ret
}

fn visit_a_rule(
    language_name: &str,
    rule_content: &Value,
    names: &mut Vec<String>,
    constant: &mut Constant,
) -> Result<()> {
    let map = rule_content
        .as_object()
        .ok_or_else(|| error("rule content must be object"))?;
    let value_type = map.get(TYPE).ok_or_else(|| error("must have type"))?;
    let type_name = value_type
        .as_str()
        .ok_or_else(|| error("type must be string"))?;
    let mut node_name = type_name.to_string();
    names.push(node_name.clone());
    match type_name {
        SEQ => {
            let value_members = map
                .get(MEMBERS)
                .ok_or_else(|| error("SEQ type must have members"))?;
            let members = value_members
                .as_array()
                .ok_or_else(|| error("members must be array"))?;
            for (i, m) in members.iter().enumerate() {
                let value_member = m
                    .as_object()
                    .ok_or_else(|| error("member must be object"))?;
                let name = if let Some(v_name) = value_member.get(language_name) {
                    v_name
                        .as_str()
                        .ok_or_else(|| error("name must be string"))?
                        .to_string()
                } else if let Some(v_type) = value_member.get(TYPE) {
                    v_type
                        .as_str()
                        .ok_or_else(|| error("type must be string"))?
                        .to_string()
                } else {
                    return Err(error("member must have a type"));
                };
                names.push(name);
                let formated_name = format_name(names);
                names.pop();
                let opt_value = constant.seq_index.get_mut(&formated_name);
                if let Some(vec) = opt_value {
                    if !vec.contains(&i) {
                        vec.push(i);
                    }
                } else {
                    constant.seq_index.insert(formated_name, vec![i]);
                }
                visit_a_rule(language_name, m, names, constant)?;
            }
        }
        CHOICE => {
            let value_members = map
                .get(MEMBERS)
                .ok_or_else(|| error("CHOICE type must have members"))?;
            let members = value_members
                .as_array()
                .ok_or_else(|| error("members must be array"))?;
            for m in members.iter() {
                visit_a_rule(language_name, m, names, constant)?;
            }
        }
        FIELD => {
            let value_content = map
                .get(CONTENT)
                .ok_or_else(|| error("FIELD type must have content"))?;
            let value_name = map.get(NAME).ok_or_else(|| error("field must have name"))?;
            let field_name = value_name
                .as_str()
                .ok_or_else(|| error("name must be string"))?
                .to_string();
            constant.field_name.insert(field_name);
            visit_a_rule(language_name, value_content, names, constant)?;
        }
        ALIAS => {
            let value_content = map
                .get(CONTENT)
                .ok_or_else(|| error("ALIAS type must have content"))?;
            let value_name = map
                .get(VALUE)
                .ok_or_else(|| error("ALIAS type must have value"))?;
            let value_name = value_name
                .as_str()
                .ok_or_else(|| error("alias value must be string"))?;
            if contains_only_alphanumeric_or_underscore(value_name) {
                constant.node_name.insert(value_name.to_string());
            }
            visit_a_rule(language_name, value_content, names, constant)?;
        }
        REPEAT | REPEAT1 | PREC | PREC_LEFT | PREC_RIGHT => {
            let value_content = map
                .get(CONTENT)
                .ok_or_else(|| error("REPEAT type must have content"))?;
            visit_a_rule(language_name, value_content, names, constant)?;
        }
        _ => {
            let opt = map.get(language_name);
            if let Some(name) = opt {
                node_name = name
                    .as_str()
                    .ok_or_else(|| error("name must be string"))?
                    .to_string();
                names.pop();
                names.push(node_name);
            }
        }
    }
    names.pop();
    Ok(())
}

fn contains_only_alphanumeric_or_underscore(value: &str) -> bool {
    value
        .chars()
        .all(|character| character == '_' || character.is_ascii_alphanumeric())
}

fn visit_rule(language_name: &str, json: Value, constant: &mut Constant) -> Result<()> {
    let map = json
        .as_object()
        .ok_or_else(|| error("json must be object"))?;
    let value_rules = map.get(RULES).ok_or_else(|| error("rules missing"))?;
    let map_rules = value_rules
        .as_object()
        .ok_or_else(|| error("rules value as object failed"))?;
    for (key, value) in map_rules.iter() {
        let mut names = vec![key.clone()];
        constant.node_name.insert(key.clone());
        visit_a_rule(language_name, value, &mut names, constant)?;
    }
    Ok(())
}

fn output_rust_file(language: &Language, path: &Path, constant: &Constant) -> Result<()> {
    let mut node_kind_id: Vec<(String, u16)> = constant
        .node_name
        .iter()
        .map(|k| {
            let id = language.id_for_node_kind(k, true);
            (k.clone(), id)
        })
        .collect();
    node_kind_id.sort();

    let mut field_name: Vec<String> = constant.field_name.iter().cloned().collect();
    field_name.sort();

    let mut seq_index: Vec<(String, Vec<usize>)> = constant
        .seq_index
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    seq_index.sort();

    let path_buf = PathBuf::from(path);
    let mut path_field_names = path_buf.clone();
    let mut path_field_ids = path_buf.clone();
    let mut path_kind_name_ids = path_buf.clone();
    let mut path_kind_names = path_buf.clone();
    let mut path_seq_index = path_buf.clone();

    path_field_names.push("ts_field_name.rs");
    path_field_ids.push("ts_field_id.rs");
    path_kind_name_ids.push("ts_kind_id.rs");
    path_kind_names.push("ts_kind_name.rs");
    path_seq_index.push("ts_seq_index.rs");

    let mut file_kind_name_ids = SFile::create(path_kind_name_ids)?;
    let mut file_kind_names = SFile::create(path_kind_names)?;
    let mut file_field_names = SFile::create(path_field_names)?;
    let mut file_field_ids = SFile::create(path_field_ids)?;
    let mut file_seq_index = SFile::create(path_seq_index)?;

    file_kind_name_ids.write_fmt(format_args!("{}", TS_CONST_COMMENTS))?;
    file_kind_name_ids.write_fmt(format_args!("// kind id of Node\n\n"))?;

    file_kind_names.write_fmt(format_args!("{}", TS_CONST_COMMENTS))?;
    file_kind_names.write_fmt(format_args!("// kind name of Node\n\n"))?;
    for (name, id) in node_kind_id {
        let mut var_name = name.clone();
        let mut name_str = name.clone();

        var_name.make_ascii_uppercase();
        file_kind_name_ids.write_fmt(format_args!("pub const {}: u16 = {};\n", var_name, id))?;

        name_str.make_ascii_lowercase();
        file_kind_names.write_fmt(format_args!(
            "pub const S_{}: &str = \"{}\";\n",
            var_name, name_str
        ))?;
    }

    file_field_names.write_fmt(format_args!("{}", TS_CONST_COMMENTS))?;
    file_field_names.write_fmt(format_args!("// field name\n\n"))?;
    file_field_ids.write_fmt(format_args!("{}", TS_CONST_COMMENTS))?;
    file_field_ids.write_fmt(format_args!("// field id\n\n"))?;
    for field_name in field_name {
        let mut upper_case_name = field_name.clone();
        upper_case_name.make_ascii_uppercase();
        file_field_names.write_fmt(format_args!(
            "pub const {}: &str = \"{}\";\n",
            upper_case_name, field_name
        ))?;

        let opt_id = language.field_id_for_name(&field_name);

        if let Some(id) = opt_id {
            file_field_ids.write_fmt(format_args!(
                "pub const FI_{}: u16 = {};\n",
                upper_case_name, id
            ))?;
        }
    }

    file_seq_index.write_fmt(format_args!("{}", TS_CONST_COMMENTS))?;
    file_seq_index.write_fmt(format_args!("// sequence index in array of SEQ type\n\n"))?;
    for (name, index) in seq_index {
        let mut name = name;
        name.make_ascii_uppercase();
        if index.len() == 1 {
            let i = index[0];
            file_seq_index.write_fmt(format_args!("pub const {}: usize = {};\n", name, i))?;
            continue;
        }
        for i in index {
            file_seq_index.write_fmt(format_args!("pub const {}_{}: usize = {};\n", name, i, i))?;
        }
    }
    Ok(())
}
