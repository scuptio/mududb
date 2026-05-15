use anyhow::{Context, Result, anyhow, bail};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::config::Config;
use crate::util::{clean_dir, run_command};

pub(crate) fn check_system_libraries(
    cfg: &Config,
    archive_path: &Path,
    version: &str,
) -> Result<()> {
    let tmp_dir = cfg.root.join("tmp").join(format!("{version}.lib-check"));
    clean_dir(&tmp_dir)?;
    fs::create_dir_all(&tmp_dir)?;

    run_command(
        Command::new("tar")
            .arg("-xzf")
            .arg(archive_path)
            .arg("-C")
            .arg(&tmp_dir)
            .arg(format!("{version}/lib/lib-list.txt")),
    )
    .with_context(|| format!("extract lib-list.txt from {}", archive_path.display()))?;

    let lib_list = tmp_dir.join(version).join("lib").join("lib-list.txt");
    let missing = missing_libraries(&lib_list)?;
    clean_dir(&tmp_dir)?;

    if !missing.is_empty() {
        bail!(
            "missing required dynamic libraries: {}. Install the matching OS packages before activation",
            missing.join(", ")
        );
    }

    Ok(())
}

fn missing_libraries(lib_list: &Path) -> Result<Vec<String>> {
    if !lib_list.exists() {
        return Ok(Vec::new());
    }

    let text = fs::read_to_string(lib_list)?;
    let mut missing = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let name = line
            .split_whitespace()
            .next()
            .ok_or_else(|| anyhow!("invalid lib-list line: {line}"))?;
        if is_os_runtime_library(name) {
            continue;
        }
        if !system_library_exists(name) {
            missing.push(name.to_string());
        }
    }
    Ok(missing)
}

fn is_os_runtime_library(name: &str) -> bool {
    name.starts_with("libc.so")
        || name.starts_with("libm.so")
        || name.starts_with("ld-linux")
        || name.starts_with("ld-musl")
}

fn system_library_exists(name: &str) -> bool {
    let paths = [
        PathBuf::from("/lib"),
        PathBuf::from("/lib64"),
        PathBuf::from("/usr/lib"),
        PathBuf::from("/usr/lib64"),
        PathBuf::from("/lib/x86_64-linux-gnu"),
        PathBuf::from("/usr/lib/x86_64-linux-gnu"),
    ];
    if paths.iter().any(|dir| dir.join(name).exists()) {
        return true;
    }

    Command::new("ldconfig")
        .arg("-p")
        .output()
        .ok()
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .is_some_and(|stdout| stdout.contains(name))
}
