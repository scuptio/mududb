use anyhow::{Result, anyhow, bail};
use std::ffi::OsStr;
use std::fs;
use std::path::Path;

use crate::config::{Config, TOOL_BINARIES};
use crate::util::{clean_dir, create_symlink, remove_path};

pub(crate) fn ensure_layout(root: &Path) -> Result<()> {
    fs::create_dir_all(root.join("downloads"))?;
    fs::create_dir_all(root.join("tmp"))?;
    fs::create_dir_all(root.join("bin"))?;
    fs::create_dir_all(root.join("toolchains"))?;

    let settings = root.join("settings.toml");
    if !settings.exists() {
        fs::write(
            settings,
            format!("manifest_version = 1\nroot = \"{}\"\n", root.display()),
        )?;
    }

    Ok(())
}

pub(crate) fn activate_toolchain(cfg: &Config, version: &str) -> Result<()> {
    let toolchain = cfg.root.join("toolchains").join(version);
    if !toolchain.is_dir() {
        bail!("toolchain does not exist: {}", toolchain.display());
    }

    let current = cfg.root.join("toolchains").join("current");
    let next = cfg.root.join("toolchains").join(".current-next");
    remove_path(&next)?;
    create_symlink(Path::new(version), &next)?;
    remove_path(&current)?;
    fs::rename(&next, &current)?;
    Ok(())
}

pub(crate) fn refresh_proxies(cfg: &Config) -> Result<()> {
    let bin_dir = cfg.root.join("bin");
    fs::create_dir_all(&bin_dir)?;
    for bin in TOOL_BINARIES {
        let proxy = bin_dir.join(bin);
        remove_path(&proxy)?;
        let target = Path::new("..")
            .join("toolchains")
            .join("current")
            .join("bin")
            .join(bin);
        create_symlink(&target, &proxy)?;
    }
    Ok(())
}

pub(crate) fn list_toolchains(cfg: &Config) -> Result<()> {
    let toolchains_dir = cfg.root.join("toolchains");
    if !toolchains_dir.exists() {
        println!("no toolchains installed");
        return Ok(());
    }

    let current = current_version(cfg).ok();
    for entry in fs::read_dir(toolchains_dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();
        if name == "current" || name.starts_with('.') {
            continue;
        }
        if entry.path().is_dir() {
            if current.as_deref() == Some(name.as_str()) {
                println!("{name} (current)");
            } else {
                println!("{name}");
            }
        }
    }
    Ok(())
}

pub(crate) fn uninstall(cfg: &Config, version: &str) -> Result<()> {
    let current = current_version(cfg).ok();
    if current.as_deref() == Some(version) {
        bail!("cannot uninstall current toolchain {version}; install another version first");
    }

    let dir = cfg.root.join("toolchains").join(version);
    if !dir.exists() {
        bail!("toolchain {version} is not installed");
    }
    clean_dir(&dir)?;
    println!("uninstalled {version}");
    Ok(())
}

fn current_version(cfg: &Config) -> Result<String> {
    let path = fs::read_link(cfg.root.join("toolchains").join("current"))?;
    path.file_name()
        .and_then(OsStr::to_str)
        .map(str::to_string)
        .ok_or_else(|| anyhow!("current toolchain link is invalid"))
}

pub(crate) fn print_path_hint(cfg: &Config) {
    let bin_dir = cfg.root.join("bin");
    if std::env::var_os("PATH")
        .and_then(|path| {
            std::env::split_paths(&path)
                .any(|item| item == bin_dir)
                .then_some(())
        })
        .is_none()
    {
        println!("add {} to PATH", bin_dir.display());
    }
}
