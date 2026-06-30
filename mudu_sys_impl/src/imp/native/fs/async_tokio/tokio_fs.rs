use mudu::common::result::RS;
use mudu::error::others::io_error_with_message;
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
        io_error_with_message(e, "create tokio directory error")
    })?;
    debug!("tokio_create_dir_all end {}", path.as_ref().display());
    Ok(())
}

pub(crate) async fn metadata_len(path: impl AsRef<Path>) -> RS<u64> {
    fs::metadata(path)
        .await
        .map(|metadata| metadata.len())
        .map_err(|e| io_error_with_message(e, "read tokio metadata error"))
}

pub(crate) async fn path_exists(path: impl AsRef<Path>) -> RS<bool> {
    fs::try_exists(path)
        .await
        .map_err(|e| io_error_with_message(e, "check tokio path exists error"))
}

pub(crate) async fn remove_file_if_exists(path: impl AsRef<Path>) -> RS<()> {
    match fs::remove_file(path).await {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == ErrorKind::NotFound => Ok(()),
        Err(err) => Err(io_error_with_message(err, "remove tokio file error")),
    }
}

pub(crate) async fn read_dir(path: impl AsRef<Path>) -> RS<Vec<PathBuf>> {
    let mut paths = Vec::new();
    let mut entries = fs::read_dir(path)
        .await
        .map_err(|e| io_error_with_message(e, "read tokio directory error"))?;
    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|e| io_error_with_message(e, "read tokio directory entry error"))?
    {
        paths.push(entry.path());
    }
    Ok(paths)
}
