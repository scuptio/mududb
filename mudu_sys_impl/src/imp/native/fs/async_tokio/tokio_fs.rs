use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use tokio::fs;

use crate::scoped_task_trace;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use tracing::{debug, info};

pub(crate) async fn create_dir_all(path: impl AsRef<Path>) -> RS<()> {
    debug!("tokio_create_dir_all {}", path.as_ref().display());
    scoped_task_trace!();
    fs::create_dir_all(path.as_ref()).await.map_err(|e| {
        info!("crate dir all error {}", path.as_ref().display());
        m_error!(EC::IOErr, "create tokio directory error", e)
    })?;
    debug!("tokio_create_dir_all end {}", path.as_ref().display());
    Ok(())
}

pub(crate) async fn metadata_len(path: impl AsRef<Path>) -> RS<u64> {
    fs::metadata(path)
        .await
        .map(|metadata| metadata.len())
        .map_err(|e| m_error!(EC::IOErr, "read tokio metadata error", e))
}

pub(crate) async fn path_exists(path: impl AsRef<Path>) -> RS<bool> {
    fs::try_exists(path)
        .await
        .map_err(|e| m_error!(EC::IOErr, "check tokio path exists error", e))
}

pub(crate) async fn remove_file_if_exists(path: impl AsRef<Path>) -> RS<()> {
    match fs::remove_file(path).await {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == ErrorKind::NotFound => Ok(()),
        Err(err) => Err(m_error!(EC::IOErr, "remove tokio file error", err)),
    }
}

pub(crate) async fn read_dir(path: impl AsRef<Path>) -> RS<Vec<PathBuf>> {
    let mut paths = Vec::new();
    let mut entries = fs::read_dir(path)
        .await
        .map_err(|e| m_error!(EC::IOErr, "read tokio directory error", e))?;
    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|e| m_error!(EC::IOErr, "read tokio directory entry error", e))?
    {
        paths.push(entry.path());
    }
    Ok(paths)
}

#[allow(unused)]
pub(crate) async fn remove_dir_all(path: impl AsRef<Path>) -> RS<()> {
    fs::remove_dir_all(path)
        .await
        .map_err(|e| m_error!(EC::IOErr, "remove tokio directory error", e))
}

#[allow(unused)]
pub(crate) async fn write_all(path: impl AsRef<Path>, data: &[u8]) -> RS<()> {
    fs::write(path, data)
        .await
        .map_err(|e| m_error!(EC::IOErr, "write tokio file error", e))
}
