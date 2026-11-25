//! Module-level dependency and coupling analyzer.
//!
//! Parses a crate's module tree and `use` statements with `syn` and reports
//! the module-level dependency graph: strongly-connected components (cycles),
//! dependency hubs (in/out degree), Robert Martin coupling metrics, unnecessary
//! `pub` visibility and orphaned source files.

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
#![deny(clippy::dbg_macro)]

mod analyze;
mod collect;
mod orphans;
mod pub_vis;
mod report;

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use cargo_metadata::MetadataCommand;
use clap::{Parser, Subcommand};

/// Command-line interface.
#[derive(Debug, Parser)]
#[command(
    name = "mudu_arch",
    about = "Analyze a crate's internal module dependency graph"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Dependency graph: cycles, hubs and coupling metrics.
    Report {
        /// Path to the crate's `Cargo.toml`.
        #[arg(long, default_value = "Cargo.toml")]
        manifest: PathBuf,
        /// Include `#[cfg(test)]` modules and uses.
        #[arg(long)]
        cfg_test: bool,
        /// Emit a machine-readable JSON report.
        #[arg(long)]
        json: bool,
        /// Emit the dependency structure matrix as JSON.
        #[arg(long)]
        dsm: bool,
        /// Exit non-zero when a cycle or god module is found (CI gate).
        #[arg(long)]
        deny: bool,
    },
    /// Suggest `pub` visibility downgrades for items no other crate consumes.
    PubVisibility {
        #[arg(long, default_value = "Cargo.toml")]
        manifest: PathBuf,
        #[arg(long)]
        cfg_test: bool,
        /// Save the public item inventory to a JSON snapshot file.
        #[arg(long)]
        save: Option<PathBuf>,
        /// Diff the public item inventory against a JSON snapshot file.
        #[arg(long)]
        diff: Option<PathBuf>,
    },
    /// Report `.rs` files not reachable from any `mod` declaration.
    Orphans {
        #[arg(long, default_value = "Cargo.toml")]
        manifest: PathBuf,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Report {
            manifest,
            cfg_test,
            json,
            dsm,
            deny,
        } => cmd_report(&manifest, cfg_test, json, dsm, deny),
        Command::PubVisibility {
            manifest,
            cfg_test,
            save,
            diff,
        } => cmd_pub_visibility(&manifest, cfg_test, save, diff),
        Command::Orphans { manifest } => cmd_orphans(&manifest),
    }
}

fn cmd_report(manifest: &Path, cfg_test: bool, json: bool, dsm: bool, deny: bool) -> Result<()> {
    let (crate_name, src_path) = locate_lib_src(manifest)?;
    let src_dir = src_path
        .parent()
        .context("lib source path has no parent directory")?
        .to_path_buf();

    let (nodes, _files) = collect::collect_modules(&src_path, &crate_name, &src_dir, cfg_test)?;
    let modules: HashSet<String> = nodes.iter().map(|node| node.path.clone()).collect();
    let uses = collect::collect_uses(&nodes, &modules, &crate_name, cfg_test);
    let analysis = analyze::analyze(&nodes, &modules, &uses);
    let extra = report::ExtraMetrics {
        cohesion: analyze::cohesion(&nodes),
        deep: analyze::deep_nesting(&modules, 4),
        unstable: analyze::unstable_dependencies(&uses, &analysis.instability),
        cycle_break: analyze::cycle_break_edges(&uses, &analysis.cycles),
        pub_surface: collect::collect_pub_surface(&nodes),
        danger: collect::collect_danger_calls(&nodes),
    };

    if dsm {
        report::print_dsm(&uses)?;
    } else if json {
        report::print_json(&crate_name, &nodes, &uses, &analysis, &extra)?;
    } else {
        report::print_text(&crate_name, &nodes, &uses, &analysis, &extra);
    }

    if deny {
        let violations = analysis
            .cycles
            .iter()
            .filter(|cycle| cycle.len() > 1)
            .count()
            + nodes.iter().filter(|node| node.item_count >= 60).count();
        if violations > 0 {
            return Err(anyhow::anyhow!(
                "deny: {violations} architecture violation(s) (cycle or god module)"
            ));
        }
    }
    Ok(())
}

fn cmd_pub_visibility(
    manifest: &Path,
    cfg_test: bool,
    save: Option<PathBuf>,
    diff: Option<PathBuf>,
) -> Result<()> {
    if let Some(save_path) = save {
        let paths = pub_vis::collect_pub_item_paths(manifest, cfg_test)?;
        let json = serde_json::to_string_pretty(&paths)?;
        mudu_sys::fs::sync::sync_write(&save_path, json.as_bytes())
            .with_context(|| format!("write snapshot {}", save_path.display()))?;
        println!("wrote {} pub items to {}", paths.len(), save_path.display());
        return Ok(());
    }
    if let Some(diff_path) = diff {
        let current = pub_vis::collect_pub_item_paths(manifest, cfg_test)?;
        let snapshot_text = mudu_sys::fs::sync::sync_read_to_string(&diff_path)
            .with_context(|| format!("read snapshot {}", diff_path.display()))?;
        let snapshot: Vec<String> = serde_json::from_str(&snapshot_text)
            .with_context(|| format!("parse snapshot {}", diff_path.display()))?;
        let snapshot_set: HashSet<&str> = snapshot.iter().map(String::as_str).collect();
        let current_set: HashSet<&str> = current.iter().map(String::as_str).collect();
        let mut added: Vec<&String> = current
            .iter()
            .filter(|path| !snapshot_set.contains(path.as_str()))
            .collect();
        let mut removed: Vec<&String> = snapshot
            .iter()
            .filter(|path| !current_set.contains(path.as_str()))
            .collect();
        added.sort();
        removed.sort();
        for path in &added {
            println!("+ {path}");
        }
        for path in &removed {
            println!("- {path}");
        }
        if added.is_empty() && removed.is_empty() {
            println!("no public API changes");
        }
        return Ok(());
    }

    let suggestions = pub_vis::analyze_pub_visibility(manifest, cfg_test)?;
    if suggestions.is_empty() {
        println!("no unnecessary `pub` items");
        return Ok(());
    }
    for suggestion in suggestions {
        println!(
            "{}: {} -> {}",
            suggestion.item, suggestion.current, suggestion.suggested
        );
    }
    Ok(())
}

fn cmd_orphans(manifest: &Path) -> Result<()> {
    let (crate_name, src_path) = locate_lib_src(manifest)?;
    let src_dir = src_path
        .parent()
        .context("lib source path has no parent directory")?
        .to_path_buf();
    let (_nodes, files) = collect::collect_modules(&src_path, &crate_name, &src_dir, false)?;
    let orphaned = orphans::find_orphans(&src_dir, &files)?;
    if orphaned.is_empty() {
        println!("no orphaned source files");
        return Ok(());
    }
    for file in orphaned {
        println!("{}", file.display());
    }
    Ok(())
}

/// Locates the crate's library source path and Rust crate name from its
/// manifest. Falls back to the first binary target when there is no library.
fn locate_lib_src(manifest: &Path) -> Result<(String, PathBuf)> {
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
    Ok((
        target.name.clone(),
        target.src_path.clone().into_std_path_buf(),
    ))
}
