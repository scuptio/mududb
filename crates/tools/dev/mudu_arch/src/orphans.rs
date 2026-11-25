//! Orphaned source file detection: `.rs` files present under `src/` that are
//! not reachable from any `mod` declaration.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::Result;

/// Finds `.rs` files under `src_dir` that are not in `referenced`.
pub(crate) fn find_orphans(src_dir: &Path, referenced: &HashSet<PathBuf>) -> Result<Vec<PathBuf>> {
    let mut all = Vec::new();
    walk(src_dir, &mut all)?;
    let mut orphans: Vec<PathBuf> = all
        .into_iter()
        .filter(|file| !referenced.contains(file))
        .collect();
    orphans.sort();
    Ok(orphans)
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
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
            walk(&entry.path(), out)?;
        } else if file_type.is_file()
            && entry.path().extension().and_then(|e| e.to_str()) == Some("rs")
        {
            out.push(entry.path());
        }
    }
    Ok(())
}
