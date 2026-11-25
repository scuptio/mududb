//! Source parsing: module tree, dependency edges, item statistics and public
//! item inventory, all via `syn`.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use syn::visit::Visit;
use syn::{Item, UseTree};

/// A strongly-connected component (a set of mutually-dependent modules).
pub(crate) type Cycle = Vec<String>;
/// In/out degree map keyed by module path.
pub(crate) type DegreeMap = HashMap<String, usize>;

/// One module's own top-level items (its submodules are separate nodes).
pub(crate) struct ModuleNode {
    pub path: String,
    /// The source file defining this module.
    pub file: PathBuf,
    pub items: Vec<Item>,
    /// Number of abstract items (`trait`).
    pub abstract_count: usize,
    /// Number of concrete type items (`struct` / `enum` / `union`).
    pub concrete_count: usize,
    /// Total number of owned items (including functions and impls).
    pub item_count: usize,
}

/// A public item's visibility and its fully-qualified path.
pub(crate) struct PubItem {
    pub path: String,
    pub visibility: Visibility,
}

/// The declared visibility of an item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Visibility {
    Pub,
    PubCrate,
    PubSuper,
    PubInPath,
    Private,
}

/// Collects every module in the crate, resolving the module tree (inline
/// modules, file-based modules, `#[path]` and `#[cfg(test)]` gating). Returns
/// the module nodes plus every referenced source file.
pub(crate) fn collect_modules(
    src_path: &Path,
    crate_name: &str,
    src_dir: &Path,
    cfg_test: bool,
) -> Result<(Vec<ModuleNode>, HashSet<PathBuf>)> {
    let content = mudu_sys::fs::sync::sync_read_to_string(src_path)
        .with_context(|| format!("read {}", src_path.display()))?;
    let ast = syn::parse_file(&content).with_context(|| format!("parse {}", src_path.display()))?;
    let mut nodes = Vec::new();
    let mut files = HashSet::new();
    files.insert(src_path.to_path_buf());
    process_items(
        crate_name,
        &ModuleDirs {
            file_path: src_path.to_path_buf(),
            file_dir: src_dir.to_path_buf(),
            submodule_dir: src_dir.to_path_buf(),
        },
        ast.items,
        cfg_test,
        &mut nodes,
        &mut files,
    )?;
    Ok((nodes, files))
}

/// A module's backing file and its child-module directories.
#[derive(Clone)]
struct ModuleDirs {
    file_path: PathBuf,
    file_dir: PathBuf,
    submodule_dir: PathBuf,
}

/// Recursively processes a module's items, pushing one `ModuleNode` per module.
///
/// `dirs.file_path` is the source file defining this module; `dirs.file_dir` is
/// the directory containing it (used to resolve `#[path]`), while
/// `dirs.submodule_dir` is the directory in which this module's out-of-line
/// child modules live. The latter two differ for a `foo.rs`-style module.
fn process_items(
    module_path: &str,
    dirs: &ModuleDirs,
    items: Vec<Item>,
    cfg_test: bool,
    nodes: &mut Vec<ModuleNode>,
    files: &mut HashSet<PathBuf>,
) -> Result<()> {
    let mut own_items = Vec::new();
    for item in items {
        if let Item::Mod(module) = item {
            if has_cfg_test(&module.attrs) && !cfg_test {
                continue;
            }
            let child = format!("{module_path}::{}", module.ident);
            if let Some((_, inline_items)) = module.content {
                let child_dirs = ModuleDirs {
                    file_path: dirs.file_path.clone(),
                    file_dir: dirs.file_dir.clone(),
                    submodule_dir: dirs.submodule_dir.join(module.ident.to_string()),
                };
                process_items(&child, &child_dirs, inline_items, cfg_test, nodes, files)?;
            } else {
                let (child_file, child_dir) = resolve_module_file(
                    &module,
                    &dirs.file_dir,
                    &dirs.submodule_dir,
                    &module.ident.to_string(),
                )?;
                files.insert(child_file.clone());
                let child_content = mudu_sys::fs::sync::sync_read_to_string(&child_file)
                    .with_context(|| format!("read {}", child_file.display()))?;
                if is_generated(&child_content) {
                    nodes.push(ModuleNode {
                        path: child,
                        file: child_file.clone(),
                        items: Vec::new(),
                        abstract_count: 0,
                        concrete_count: 0,
                        item_count: 0,
                    });
                    continue;
                }
                let child_ast = syn::parse_file(&child_content)
                    .with_context(|| format!("parse {}", child_file.display()))?;
                let child_file_dir = child_file.parent().with_context(|| {
                    format!("module file {} has no parent", child_file.display())
                })?;
                let child_dirs = ModuleDirs {
                    file_path: child_file.clone(),
                    file_dir: child_file_dir.to_path_buf(),
                    submodule_dir: child_dir,
                };
                process_items(&child, &child_dirs, child_ast.items, cfg_test, nodes, files)?;
            }
        } else {
            own_items.push(item);
        }
    }
    let mut abstract_count = 0;
    let mut concrete_count = 0;
    for item in &own_items {
        match item {
            Item::Trait(_) => abstract_count += 1,
            Item::Struct(_) | Item::Enum(_) | Item::Union(_) => concrete_count += 1,
            _ => {}
        }
    }
    let item_count = own_items.len();
    nodes.push(ModuleNode {
        path: module_path.to_string(),
        file: dirs.file_path.clone(),
        items: own_items,
        abstract_count,
        concrete_count,
        item_count,
    });
    Ok(())
}

/// Resolves an out-of-line module's backing file and its child-module
/// directory, honouring `#[path]`.
fn resolve_module_file(
    module: &syn::ItemMod,
    file_dir: &Path,
    submodule_dir: &Path,
    ident: &str,
) -> Result<(PathBuf, PathBuf)> {
    if let Some(path_attr) = module
        .attrs
        .iter()
        .find(|attr| attr.path().is_ident("path"))
    {
        if let syn::Meta::NameValue(name_value) = &path_attr.meta
            && let syn::Expr::Lit(expr_lit) = &name_value.value
            && let syn::Lit::Str(lit_str) = &expr_lit.lit
        {
            let file = file_dir.join(lit_str.value());
            let child_dir = if file.file_name().is_some_and(|name| name == "mod.rs") {
                file.parent()
                    .with_context(|| format!("module file {} has no parent", file.display()))?
                    .to_path_buf()
            } else {
                file.with_extension("")
            };
            return Ok((file, child_dir));
        }
        return Err(anyhow::anyhow!(
            "unsupported #[path] form on module `{}`",
            module.ident
        ));
    }
    let flat = submodule_dir.join(format!("{ident}.rs"));
    if mudu_sys::fs::sync::sync_path_exists(&flat) {
        return Ok((flat, submodule_dir.join(ident)));
    }
    let nested = submodule_dir.join(ident).join("mod.rs");
    if mudu_sys::fs::sync::sync_path_exists(&nested) {
        return Ok((nested, submodule_dir.join(ident)));
    }
    Err(anyhow::anyhow!(
        "cannot find module `{ident}` under {}",
        submodule_dir.display()
    ))
}

/// Returns whether `content` carries a generated-code marker. Two conventions
/// are recognised: the canonical `Generated by … Do not edit manually.` and the
/// tree-sitter constant tables' `generate this file` / `do not change`. Such
/// files are parsed but contribute no items, so they do not pollute
/// god-module / cohesion metrics.
pub(crate) fn is_generated(content: &str) -> bool {
    let header = content
        .lines()
        .take(10)
        .map(str::to_lowercase)
        .collect::<Vec<_>>()
        .join("\n");
    let marks_generated = header.contains("generated by")
        || header.contains("auto-generated")
        || header.contains("generate this file");
    let marks_do_not_edit = header.contains("do not edit") || header.contains("do not change");
    marks_generated && marks_do_not_edit
}

/// Returns whether `attrs` carries `#[cfg(test)]` (or `#[cfg(...)]` mentioning
/// `test`).
pub(crate) fn has_cfg_test(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attr| {
        attr.path().is_ident("cfg")
            && attr.meta.require_list().is_ok_and(|list| {
                list.tokens
                    .to_string()
                    .split(|c: char| !c.is_alphanumeric() && c != '_')
                    .any(|part| part == "test")
            })
    })
}

/// Builds the module -> referenced-modules dependency graph.
///
/// Two edge kinds are collected: explicit `use` statements and fully-qualified
/// path references in code (`crate::foo::bar()`, `super::Type`), both of which
/// are real coupling between modules.
pub(crate) fn collect_uses(
    nodes: &[ModuleNode],
    modules: &HashSet<String>,
    crate_name: &str,
    cfg_test: bool,
) -> HashMap<String, HashSet<String>> {
    let mut uses: HashMap<String, HashSet<String>> = HashMap::new();
    for node in nodes {
        let mut targets = HashSet::new();
        for item in &node.items {
            if item_has_cfg_test(item) && !cfg_test {
                continue;
            }
            if let Item::Use(use_item) = item {
                let mut paths = Vec::new();
                expand_use_tree(Vec::new(), &use_item.tree, &mut paths);
                for segments in paths {
                    if let Some(target) = resolve_target(&segments, crate_name, &node.path, modules)
                        && target != node.path
                    {
                        targets.insert(target);
                    }
                }
            }
            let mut collector = PathRefCollector {
                crate_name,
                current_module: &node.path,
                modules,
                targets: &mut targets,
            };
            collector.visit_item(item);
        }
        uses.insert(node.path.clone(), targets);
    }
    uses
}

/// Returns whether an item is gated behind `#[cfg(test)]`.
pub(crate) fn item_has_cfg_test(item: &Item) -> bool {
    let attrs: &[syn::Attribute] = match item {
        Item::Use(use_item) => &use_item.attrs,
        Item::Mod(module) => &module.attrs,
        Item::Fn(function) => &function.attrs,
        Item::Struct(struct_item) => &struct_item.attrs,
        Item::Enum(enum_item) => &enum_item.attrs,
        Item::Union(union_item) => &union_item.attrs,
        Item::Trait(trait_item) => &trait_item.attrs,
        Item::Impl(impl_item) => &impl_item.attrs,
        Item::Const(const_item) => &const_item.attrs,
        Item::Static(static_item) => &static_item.attrs,
        Item::Type(type_item) => &type_item.attrs,
        Item::Macro(macro_item) => &macro_item.attrs,
        _ => return false,
    };
    has_cfg_test(attrs)
}

/// Returns the declared visibility of an item.
pub(crate) fn item_visibility(item: &Item) -> Visibility {
    let vis: &syn::Visibility = match item {
        Item::Use(use_item) => &use_item.vis,
        Item::Mod(module) => &module.vis,
        Item::Fn(function) => &function.vis,
        Item::Struct(struct_item) => &struct_item.vis,
        Item::Enum(enum_item) => &enum_item.vis,
        Item::Union(union_item) => &union_item.vis,
        Item::Trait(trait_item) => &trait_item.vis,
        Item::Const(const_item) => &const_item.vis,
        Item::Static(static_item) => &static_item.vis,
        Item::Type(type_item) => &type_item.vis,
        Item::Macro(_) => return Visibility::Private,
        _ => return Visibility::Private,
    };
    match vis {
        syn::Visibility::Public(_) => Visibility::Pub,
        syn::Visibility::Restricted(restricted) => {
            let segments = &restricted.path.segments;
            if segments.len() == 1 {
                let ident = segments.first().map(|segment| segment.ident.to_string());
                if ident.as_deref() == Some("crate") {
                    return Visibility::PubCrate;
                }
                if ident.as_deref() == Some("super") {
                    return Visibility::PubSuper;
                }
            }
            Visibility::PubInPath
        }
        syn::Visibility::Inherited => Visibility::Private,
    }
}

/// Returns the simple name of an item (fn name, struct name, …).
pub(crate) fn item_name(item: &Item) -> Option<String> {
    match item {
        Item::Use(_) | Item::Mod(_) => None,
        Item::Fn(function) => Some(function.sig.ident.to_string()),
        Item::Struct(struct_item) => Some(struct_item.ident.to_string()),
        Item::Enum(enum_item) => Some(enum_item.ident.to_string()),
        Item::Union(union_item) => Some(union_item.ident.to_string()),
        Item::Trait(trait_item) => Some(trait_item.ident.to_string()),
        Item::Const(const_item) => Some(const_item.ident.to_string()),
        Item::Static(static_item) => Some(static_item.ident.to_string()),
        Item::Type(type_item) => Some(type_item.ident.to_string()),
        Item::Macro(macro_item) => macro_item.ident.as_ref().map(|i| i.to_string()),
        _ => None,
    }
}

/// Collects every public item of a crate, with its fully-qualified path and
/// visibility. `pub use` re-exports are skipped (they are aliases).
pub(crate) fn collect_pub_items(nodes: &[ModuleNode]) -> Vec<PubItem> {
    let mut items = Vec::new();
    for node in nodes {
        for item in &node.items {
            let visibility = item_visibility(item);
            if visibility == Visibility::Private {
                continue;
            }
            let Some(name) = item_name(item) else {
                continue;
            };
            items.push(PubItem {
                path: format!("{}::{}", node.path, name),
                visibility,
            });
        }
    }
    items
}

/// Collects fully-qualified path references (`crate::…`, `super::…`, `self::…`)
/// appearing anywhere inside an item.
struct PathRefCollector<'a> {
    crate_name: &'a str,
    current_module: &'a str,
    modules: &'a HashSet<String>,
    targets: &'a mut HashSet<String>,
}

impl<'ast> Visit<'ast> for PathRefCollector<'_> {
    fn visit_path(&mut self, path: &'ast syn::Path) {
        let segments: Vec<String> = path
            .segments
            .iter()
            .map(|segment| segment.ident.to_string())
            .collect();
        if let Some(target) = resolve_target(
            &segments,
            self.crate_name,
            self.current_module,
            self.modules,
        ) && target != self.current_module
        {
            self.targets.insert(target);
        }
        syn::visit::visit_path(self, path);
    }
}

/// Expands a `use` tree into its fully-qualified leaf paths.
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

/// Resolves a `use` path to the referenced module, handling `crate::`,
/// `self::` and `super::` prefixes and taking the longest prefix that names a
/// known module (so a `use crate::a::b::Item` edge lands on `crate::a::b`).
pub(crate) fn resolve_target(
    segments: &[String],
    crate_name: &str,
    current_module: &str,
    modules: &HashSet<String>,
) -> Option<String> {
    let mut base: Vec<String>;
    let mut index = 0;
    match segments.first().map(String::as_str) {
        Some("crate") => {
            base = vec![crate_name.to_string()];
            index = 1;
        }
        Some("self") => {
            base = current_module.split("::").map(str::to_string).collect();
            index = 1;
        }
        Some("super") => {
            base = current_module.split("::").map(str::to_string).collect();
            while index < segments.len() && segments[index] == "super" {
                base.pop()?;
                index += 1;
            }
        }
        _ => return None,
    }
    for segment in &segments[index..] {
        base.push(segment.clone());
    }
    for len in (1..=base.len()).rev() {
        let candidate = base[..len].join("::");
        if modules.contains(&candidate) {
            return Some(candidate);
        }
    }
    None
}

/// Counts how many of a module's own items reference each other (a proxy for
/// internal cohesion). Returns the number of distinct intra-module item
/// references.
pub(crate) fn collect_internal_refs(node: &ModuleNode) -> usize {
    let names: HashSet<String> = node.items.iter().filter_map(item_name).collect();
    let mut refs = HashSet::new();
    for item in &node.items {
        let mut collector = LocalRefCollector {
            names: &names,
            refs: &mut refs,
        };
        collector.visit_item(item);
    }
    refs.len()
}

/// Collects single-segment path references that name an item of the same
/// module (intra-module item coupling).
struct LocalRefCollector<'a> {
    names: &'a HashSet<String>,
    refs: &'a mut HashSet<String>,
}

impl<'ast> Visit<'ast> for LocalRefCollector<'_> {
    fn visit_path(&mut self, path: &'ast syn::Path) {
        if path.segments.len() == 1 {
            let name = path.segments[0].ident.to_string();
            if self.names.contains(&name) {
                self.refs.insert(name);
            }
        }
        syn::visit::visit_path(self, path);
    }
}

/// Collects every fully-qualified `crate::…` item path referenced anywhere in
/// the crate (not truncated to the owning module). Used to tell whether a
/// `pub(crate)` item is referenced from another module.
pub(crate) fn collect_full_path_refs(nodes: &[ModuleNode], crate_name: &str) -> HashSet<String> {
    let mut refs = HashSet::new();
    for node in nodes {
        for item in &node.items {
            if item_has_cfg_test(item) {
                continue;
            }
            if let Item::Use(use_item) = item {
                let mut paths = Vec::new();
                expand_use_tree(Vec::new(), &use_item.tree, &mut paths);
                for segments in paths {
                    if let Some(full) = full_crate_path(&segments, crate_name) {
                        refs.insert(full);
                    }
                }
            }
            let mut collector = FullPathCollector {
                crate_name,
                refs: &mut refs,
            };
            collector.visit_item(item);
        }
    }
    refs
}

/// Collects `crate::…` fully-qualified path references.
struct FullPathCollector<'a> {
    crate_name: &'a str,
    refs: &'a mut HashSet<String>,
}

impl<'ast> Visit<'ast> for FullPathCollector<'_> {
    fn visit_path(&mut self, path: &'ast syn::Path) {
        let segments: Vec<String> = path
            .segments
            .iter()
            .map(|segment| segment.ident.to_string())
            .collect();
        if let Some(full) = full_crate_path(&segments, self.crate_name) {
            self.refs.insert(full);
        }
        syn::visit::visit_path(self, path);
    }
}

/// Resolves a `crate::…` path to its fully-qualified form, keeping the trailing
/// item segment.
fn full_crate_path(segments: &[String], crate_name: &str) -> Option<String> {
    if segments.first().is_some_and(|first| first == "crate") {
        let mut base = vec![crate_name.to_string()];
        base.extend(segments[1..].iter().cloned());
        Some(base.join("::"))
    } else {
        None
    }
}

/// Per-module public item count (the module's public API surface).
pub(crate) fn collect_pub_surface(nodes: &[ModuleNode]) -> HashMap<String, usize> {
    let mut surface = HashMap::new();
    for node in nodes {
        let count = node
            .items
            .iter()
            .filter(|item| item_visibility(item) != Visibility::Private)
            .count();
        if count > 0 {
            surface.insert(node.path.clone(), count);
        }
    }
    surface
}

/// Counts of panic!/unwrap()/expect() calls in one module.
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct DangerCount {
    pub panics: usize,
    pub unwraps: usize,
    pub expects: usize,
}

impl DangerCount {
    /// Total dangerous calls.
    pub fn total(self) -> usize {
        self.panics + self.unwraps + self.expects
    }
}

/// Per-module panic!/unwrap()/expect() call counts.
pub(crate) fn collect_danger_calls(nodes: &[ModuleNode]) -> HashMap<String, DangerCount> {
    let mut map = HashMap::new();
    for node in nodes {
        let mut counter = DangerCounter::default();
        for item in &node.items {
            counter.visit_item(item);
        }
        if counter.total() > 0 {
            map.insert(
                node.path.clone(),
                DangerCount {
                    panics: counter.panics,
                    unwraps: counter.unwraps,
                    expects: counter.expects,
                },
            );
        }
    }
    map
}

/// Visits an item tree, counting `panic!`, `unwrap()` and `expect()`.
#[derive(Default)]
struct DangerCounter {
    panics: usize,
    unwraps: usize,
    expects: usize,
}

impl DangerCounter {
    fn total(&self) -> usize {
        self.panics + self.unwraps + self.expects
    }
}

impl<'ast> Visit<'ast> for DangerCounter {
    fn visit_expr_macro(&mut self, mac: &'ast syn::ExprMacro) {
        if mac.mac.path.is_ident("panic") {
            self.panics += 1;
        }
        syn::visit::visit_expr_macro(self, mac);
    }

    fn visit_expr_method_call(&mut self, call: &'ast syn::ExprMethodCall) {
        if call.method == "unwrap" {
            self.unwraps += 1;
        } else if call.method == "expect" {
            self.expects += 1;
        }
        syn::visit::visit_expr_method_call(self, call);
    }
}
