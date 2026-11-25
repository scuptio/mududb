use crate::contract::async_file::AsyncFile;
use crate::contract::async_fs::AsyncFs;
use crate::contract::file_options::FileOptions;
use crate::imp::fs::async_tokio;
use crate::imp::native::fs::async_tokio::async_tokio_file::AsyncTokioFile;
use async_trait::async_trait;
use mudu::common::result::RS;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Default)]
pub struct AsyncTokioFs;

impl AsyncTokioFs {
    pub(crate) fn new() -> Self {
        Self
    }
}
#[async_trait]
impl AsyncFs for AsyncTokioFs {
    async fn open(&self, path: &Path, options: FileOptions) -> RS<Arc<dyn AsyncFile>> {
        Ok(Arc::new(AsyncTokioFile::open(path, options).await?))
    }

    async fn create_dir_all(&self, path: &Path) -> RS<()> {
        async_tokio::create_dir_all(path).await
    }

    async fn metadata_len(&self, path: &Path) -> RS<u64> {
        async_tokio::metadata_len(path).await
    }

    async fn path_exists(&self, path: &Path) -> RS<bool> {
        async_tokio::path_exists(path).await
    }

    async fn remove_file_if_exists(&self, path: &Path) -> RS<()> {
        async_tokio::remove_file_if_exists(path).await
    }

    async fn read_dir(&self, path: &Path) -> RS<Vec<PathBuf>> {
        async_tokio::read_dir(path).await
    }

    async fn sync_dir(&self, path: &Path) -> RS<()> {
        let mut options = tokio::fs::OpenOptions::new();
        options.read(true);
        // Windows requires backup semantics to open a directory handle.
        // Flush failures are propagated rather than silently weakening durability.
        #[cfg(windows)]
        options.write(true).custom_flags(0x0200_0000);
        let file = options.open(path).await.map_err(|e| {
            mudu::error::others::io_error_with_message(e, "open directory for sync error")
        })?;
        file.sync_all()
            .await
            .map_err(|e| mudu::error::others::io_error_with_message(e, "sync directory error"))
    }

    async fn rename(&self, from: &Path, to: &Path) -> RS<()> {
        tokio::fs::rename(from, to)
            .await
            .map_err(|e| mudu::error::others::io_error_with_message(e, "rename tokio file error"))
    }

    async fn remove_dir_all(&self, path: &Path) -> RS<()> {
        async_tokio::remove_dir_all(path).await
    }
}
