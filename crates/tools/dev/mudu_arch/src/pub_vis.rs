//! Cross-crate `pub` visibility analysis.
//!
//! Scans the workspace for references to the analysed crate's public items and
//! suggests downgrading `pub` items that no other crate consumes to
//! `pub(crate)`.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use cargo_metadata::MetadataCommand;
use syn::visit::Visit;
use syn::{Item, UseTree};

use crate::collect::{
    Visibility, collect_full_path_refs, collect_modules, collect_pub_items, item_has_cfg_test,
};

/// A suggested visibility downgrade for one item.
pub(crate) struct VisibilitySuggestion {
    pub item: String,
    pub current: &'static str,
    pub suggested: &'static str,
}

/// Analyses one crate's public items against the whole workspace and returns
/// visibility downgrade suggestions.
pub(crate) fn analyze_pub_visibility(
    manifest: &Path,
    cfg_test: bool,
) -> Result<Vec<VisibilitySuggestion>> {
    let metadata = MetadataCommand::new()
        .manifest_path(manifest)
        .exec()
        .context("run `cargo metadata`")?;
    let manifest_abs = manifest
        .canonicalize()
        .with_context(|| format!("canonicalize manifest path {}", manifest.display()))?;
    let package = metadata
        .packages
        .iter()
        .find(|pkg| pkg.manifest_path.as_std_path() == manifest_abs.as_path())
        .or_else(|| metadata.root_package())
        .with_context(|| format!("package not found for {}", manifest.display()))?;

    let lib = package.targets.iter().find(|target| target.is_lib());
    let target = lib
        .or_else(|| package.targets.first())
        .with_context(|| format!("no lib or bin target found in package `{}`", package.name))?;
    let crate_name = target.name.clone();
    let src_path: PathBuf = target.src_path.clone().into_std_path_buf();
    let src_dir = src_path
        .parent()
        .context("lib source path has no parent directory")?
        .to_path_buf();

    let (nodes, _files) = collect_modules(&src_path, &crate_name, &src_dir, cfg_test)?;
    let pub_items = collect_pub_items(&nodes);
    let full_path_refs = collect_full_path_refs(&nodes, &crate_name);

    let workspace_members: HashSet<&cargo_metadata::PackageId> =
        metadata.workspace_members.iter().collect();
    let mut external_paths: HashSet<String> = HashSet::new();
    let mut external_names: HashSet<String> = HashSet::new();
    for other in metadata.packages.iter().filter(|pkg| {
        workspace_members.contains(&pkg.id)
            && pkg.manifest_path.as_std_path() != manifest_abs.as_path()
    }) {
        let Some(other_lib) = other.targets.iter().find(|target| target.is_lib()) else {
            continue;
        };
        let other_src: PathBuf = other_lib.src_path.clone().into_std_path_buf();
        collect_external_refs(
            &other_src,
            &crate_name,
            &mut external_paths,
            &mut external_names,
        );
    }

    let mut suggestions = Vec::new();
    for item in pub_items {
        let last_name = item.path.rsplit("::").next().unwrap_or("");
        match item.visibility {
            Visibility::Pub => {
                if !external_paths.contains(&item.path) && !external_names.contains(last_name) {
                    suggestions.push(VisibilitySuggestion {
                        item: item.path,
                        current: "pub",
                        suggested: "pub(crate)",
                    });
                }
            }
            Visibility::PubCrate if !full_path_refs.contains(&item.path) => {
                suggestions.push(VisibilitySuggestion {
                    item: item.path,
                    current: "pub(crate)",
                    suggested: "private",
                });
            }
            _ => {}
        }
    }
    suggestions.sort_by(|a, b| a.item.cmp(&b.item));
    Ok(suggestions)
}

/// Collects all fully-qualified paths starting with `target_crate` that appear
/// in the source files of `crate_src`'s crate, plus the trailing item names.
fn collect_external_refs(
    crate_src: &Path,
    target_crate: &str,
    out_paths: &mut HashSet<String>,
    out_names: &mut HashSet<String>,
) {
    let Some(src_dir) = crate_src.parent() else {
        return;
    };
    let mut files = Vec::new();
    let _ = walk_rs(src_dir, &mut files);
    for file in files {
        let Ok(content) = mudu_sys::fs::sync::sync_read_to_string(&file) else {
            continue;
        };
        let Ok(ast) = syn::parse_file(&content) else {
            continue;
        };
        for item in &ast.items {
            collect_item_refs(item, target_crate, out_paths, out_names);
        }
    }
}

fn collect_item_refs(
    item: &Item,
    target_crate: &str,
    out_paths: &mut HashSet<String>,
    out_names: &mut HashSet<String>,
) {
    if item_has_cfg_test(item) {
        return;
    }
    if let Item::Use(use_item) = item {
        let mut paths = Vec::new();
        expand_use_tree(Vec::new(), &use_item.tree, &mut paths);
        for segments in paths {
            record_if_external(&segments, target_crate, out_paths, out_names);
        }
    }
    let mut collector = ExternalPathCollector {
        target_crate,
        out_paths,
        out_names,
    };
    collector.visit_item(item);
}

struct ExternalPathCollector<'a> {
    target_crate: &'a str,
    out_paths: &'a mut HashSet<String>,
    out_names: &'a mut HashSet<String>,
}

impl<'ast> Visit<'ast> for ExternalPathCollector<'_> {
    fn visit_path(&mut self, path: &'ast syn::Path) {
        let segments: Vec<String> = path
            .segments
            .iter()
            .map(|segment| segment.ident.to_string())
            .collect();
        record_if_external(&segments, self.target_crate, self.out_paths, self.out_names);
        syn::visit::visit_path(self, path);
    }
}

fn record_if_external(
    segments: &[String],
    target_crate: &str,
    out_paths: &mut HashSet<String>,
    out_names: &mut HashSet<String>,
) {
    if segments.first().is_some_and(|first| first == target_crate) {
        out_paths.insert(segments.join("::"));
        if let Some(last) = segments.last() {
            out_names.insert(last.clone());
        }
    }
}

/// Expands a `use` tree into fully-qualified leaf paths.
fn expand_use_tree(prefix: Vec<String>, tree: &UseTree, out: &mut Vec<Vec<String>>) {
    match tree {
        UseTree::Path(path) => {
            let mut prefix = prefix;
            prefix.push(path.ident.to_string());
            expand_use_tree(prefix, &path.tree, out);
        }
        UseTree::Name(name) => {
            let mut prefix = prefix;
            prefix.push(name.ident.to_string());
            out.push(prefix);
        }
        UseTree::Rename(rename) => {
            let mut prefix = prefix;
            prefix.push(rename.ident.to_string());
            out.push(prefix);
        }
        UseTree::Glob(_) => {
            out.push(prefix);
        }
        UseTree::Group(group) => {
            for item in &group.items {
                expand_use_tree(prefix.clone(), item, out);
            }
        }
    }
}

/// Recursively collects `.rs` files, ignoring build artifacts and hidden dirs.
fn walk_rs(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    for entry in mudu_sys::fs::sync::sync_read_dir_entries(dir)? {
        let file_type = entry.file_type()?;
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            let name = entry.file_name();
            let name = name.to_str().unwrap_or("");
            if name == "target" || name.starts_with('.') {
                continue;
            }
            walk_rs(&entry.path(), out)?;
        } else if file_type.is_file()
            && entry.path().extension().and_then(|e| e.to_str()) == Some("rs")
        {
            out.push(entry.path());
        }
    }
    Ok(())
}

/// Collects the sorted fully-qualified paths of a crate's public items.
pub(crate) fn collect_pub_item_paths(manifest: &Path, cfg_test: bool) -> Result<Vec<String>> {
    let metadata = MetadataCommand::new()
        .manifest_path(manifest)
        .no_deps()
        .exec()
        .context("run `cargo metadata`")?;
    let manifest_abs = manifest
        .canonicalize()
        .with_context(|| format!("canonicalize manifest path {}", manifest.display()))?;
    let package = metadata
        .packages
        .iter()
        .find(|pkg| pkg.manifest_path.as_std_path() == manifest_abs.as_path())
        .or_else(|| metadata.root_package())
        .with_context(|| format!("package not found for {}", manifest.display()))?;
    let lib = package.targets.iter().find(|target| target.is_lib());
    let target = lib
        .or_else(|| package.targets.first())
        .with_context(|| format!("no lib or bin target found in package `{}`", package.name))?;
    let crate_name = target.name.clone();
    let src_path: PathBuf = target.src_path.clone().into_std_path_buf();
    let src_dir = src_path
        .parent()
        .context("lib source path has no parent directory")?
        .to_path_buf();

    let (nodes, _files) = collect_modules(&src_path, &crate_name, &src_dir, cfg_test)?;
    let pub_items = collect_pub_items(&nodes);
    let mut paths: Vec<String> = pub_items.into_iter().map(|item| item.path).collect();
    paths.sort();
    Ok(paths)
}
