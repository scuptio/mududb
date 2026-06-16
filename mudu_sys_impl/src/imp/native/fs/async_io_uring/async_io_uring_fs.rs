use crate::contract::async_file::AsyncFile;
use crate::contract::async_fs::AsyncFs;
use crate::contract::file_options::FileOptions;
use crate::imp::fs::async_io_uring;
use crate::imp::native::fs::async_io_uring::async_io_uring_file::AsyncIoUringFile;
use crate::scoped_task_trace;
use async_trait::async_trait;
use mudu::common::result::RS;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Default)]
pub struct AsyncIoUringFs;

impl AsyncIoUringFs {
    pub(crate) fn new() -> Self {
        Self
    }
}
#[async_trait]
impl AsyncFs for AsyncIoUringFs {
    async fn open(&self, path: &Path, options: FileOptions) -> RS<Arc<dyn AsyncFile>> {
        Ok(Arc::new(AsyncIoUringFile::open(path, options).await?))
    }

    async fn create_dir_all(&self, path: &Path) -> RS<()> {
        scoped_task_trace!();
        async_io_uring::create_dir_all(path).await
    }

    async fn metadata_len(&self, path: &Path) -> RS<u64> {
        async_io_uring::metadata_len(path).await
    }

    async fn path_exists(&self, path: &Path) -> RS<bool> {
        async_io_uring::path_exists(path).await
    }

    async fn remove_file_if_exists(&self, path: &Path) -> RS<()> {
        async_io_uring::remove_file_if_exists(path).await
    }

    async fn read_dir(&self, path: &Path) -> RS<Vec<PathBuf>> {
        let mut paths = Vec::new();
        for entry in
            std::fs::read_dir(path).map_err(|e| mudu::m_error!(mudu::error::ec::EC::IOErr, "read directory error", e))?
        {
            let entry = entry.map_err(|e| mudu::m_error!(mudu::error::ec::EC::IOErr, "read directory entry error", e))?;
            paths.push(entry.path());
        }
        Ok(paths)
    }
}
