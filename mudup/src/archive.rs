use anyhow::{Context, Result, anyhow, bail};
use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::Command;

use crate::config::{Config, TOOL_BINARIES};
use crate::util::{clean_dir, run_command};

#[derive(Debug)]
struct ArtifactManifest {
    version: String,
    bin_dir: PathBuf,
    lib_list: PathBuf,
    files: BTreeSet<PathBuf>,
}

pub(crate) fn extract_toolchain(
    cfg: &Config,
    archive_path: &Path,
    version: &str,
) -> Result<PathBuf> {
    validate_archive_paths(archive_path, version)?;

    let tmp_dir = cfg.root.join("tmp").join(version);
    let install_dir = cfg.root.join("toolchains").join(version);
    clean_dir(&tmp_dir)?;
    if install_dir.exists() {
        bail!("toolchain {version} is already installed");
    }
    fs::create_dir_all(&tmp_dir)?;

    run_command(
        Command::new("tar")
            .arg("-xzf")
            .arg(archive_path)
            .arg("-C")
            .arg(&tmp_dir),
    )
    .with_context(|| format!("extract {}", archive_path.display()))?;

    let extracted = tmp_dir.join(version);
    if !extracted.is_dir() {
        bail!("archive did not contain expected top-level directory {version}");
    }
    fs::rename(&extracted, &install_dir)?;
    clean_dir(&tmp_dir)?;
    Ok(install_dir)
}

fn validate_archive_paths(archive_path: &Path, version: &str) -> Result<()> {
    let output = Command::new("tar")
        .arg("-tzf")
        .arg(archive_path)
        .output()
        .with_context(|| format!("list archive {}", archive_path.display()))?;
    if !output.status.success() {
        bail!("tar failed to list {}", archive_path.display());
    }

    let listing = String::from_utf8(output.stdout)?;
    for entry in listing.lines() {
        validate_relative_archive_path(entry, version)?;
    }
    Ok(())
}

fn validate_relative_archive_path(entry: &str, version: &str) -> Result<()> {
    let path = Path::new(entry);
    let mut components = path.components();
    match components.next() {
        Some(Component::Normal(first)) if first == OsStr::new(version) => {}
        _ => bail!("archive entry escapes expected root: {entry}"),
    }

    for component in components {
        match component {
            Component::Normal(_) => {}
            _ => bail!("unsafe archive entry: {entry}"),
        }
    }
    Ok(())
}

pub(crate) fn validate_toolchain(install_dir: &Path) -> Result<()> {
    let manifest_path = install_dir.join("manifest.txt");
    let manifest = parse_artifact_manifest(&manifest_path)?;
    let dir_version = install_dir
        .file_name()
        .and_then(OsStr::to_str)
        .ok_or_else(|| anyhow!("invalid toolchain directory {}", install_dir.display()))?;
    if manifest.version != dir_version {
        bail!(
            "manifest version {} does not match toolchain directory {}",
            manifest.version,
            dir_version
        );
    }

    for bin in TOOL_BINARIES {
        let path = install_dir.join(&manifest.bin_dir).join(bin);
        if !path.is_file() {
            bail!("required binary missing: {}", path.display());
        }
    }

    let lib_list = install_dir.join(&manifest.lib_list);
    if !lib_list.is_file() {
        bail!("library list missing: {}", lib_list.display());
    }

    for file in &manifest.files {
        let path = install_dir.join(file);
        if !path.exists() {
            bail!("manifest file missing: {}", path.display());
        }
    }

    Ok(())
}

fn parse_artifact_manifest(path: &Path) -> Result<ArtifactManifest> {
    let text = fs::read_to_string(path)
        .with_context(|| format!("read artifact manifest {}", path.display()))?;
    let mut version = None;
    let mut bin_dir = None;
    let mut lib_list = None;
    let mut files = BTreeSet::new();
    let mut in_files = false;

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if line == "[files]" {
            in_files = true;
            continue;
        }
        if in_files {
            files.insert(PathBuf::from(line));
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            let value = value.trim();
            match key.trim() {
                "version" => version = Some(value.to_string()),
                "bin_dir" => bin_dir = Some(PathBuf::from(value)),
                "lib_list" => lib_list = Some(PathBuf::from(value)),
                _ => {}
            }
        }
    }

    Ok(ArtifactManifest {
        version: version.ok_or_else(|| anyhow!("manifest missing version"))?,
        bin_dir: bin_dir.unwrap_or_else(|| PathBuf::from("bin")),
        lib_list: lib_list.unwrap_or_else(|| PathBuf::from("lib/lib-list.txt")),
        files,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_archive_path_escape() {
        assert!(validate_relative_archive_path("v1/bin/mudud", "v1").is_ok());
        assert!(validate_relative_archive_path("/v1/bin/mudud", "v1").is_err());
        assert!(validate_relative_archive_path("v1/../bin/mudud", "v1").is_err());
        assert!(validate_relative_archive_path("other/bin/mudud", "v1").is_err());
    }

    #[test]
    fn parses_artifact_manifest() {
        let path = std::env::temp_dir().join(format!("mudup-manifest-{}.txt", std::process::id()));
        fs::write(
            &path,
            "version=v1\nbin_dir=bin\nlib_list=lib/lib-list.txt\n\n[files]\nbin/mudud\n",
        )
        .unwrap();
        let manifest = parse_artifact_manifest(&path).unwrap();
        let _ = fs::remove_file(path);
        assert_eq!(manifest.version, "v1");
        assert!(manifest.files.contains(Path::new("bin/mudud")));
    }
}
