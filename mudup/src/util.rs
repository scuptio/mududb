use anyhow::{Result, bail};
use std::fs;
use std::io;
use std::path::Path;
use std::process::Command;

pub(crate) fn clean_dir(path: &Path) -> Result<()> {
    if path.exists() {
        fs::remove_dir_all(path)?;
    }
    Ok(())
}

pub(crate) fn remove_path(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {
            fs::remove_dir_all(path)?;
        }
        Ok(_) => {
            fs::remove_file(path)?;
        }
        Err(err) if err.kind() == io::ErrorKind::NotFound => {}
        Err(err) => return Err(err.into()),
    }
    Ok(())
}

pub(crate) fn run_command(command: &mut Command) -> Result<()> {
    let status = command.status()?;
    if !status.success() {
        bail!("command failed with status {status}");
    }
    Ok(())
}

#[cfg(unix)]
pub(crate) fn create_symlink(target: &Path, link: &Path) -> Result<()> {
    std::os::unix::fs::symlink(target, link)?;
    Ok(())
}

#[cfg(windows)]
pub(crate) fn create_symlink(target: &Path, link: &Path) -> Result<()> {
    if target.is_dir() {
        std::os::windows::fs::symlink_dir(target, link)?;
    } else {
        std::os::windows::fs::symlink_file(target, link)?;
    }
    Ok(())
}
