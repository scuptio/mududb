use crate::contract::async_file::AsyncFile;
use crate::contract::file_options::FileOptions;
use async_trait::async_trait;
use mudu::common::result::RS;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[async_trait]
pub trait AsyncFs: Send + Sync {
    async fn open(&self, path: &Path, options: FileOptions) -> RS<Arc<dyn AsyncFile>>;
    async fn create_dir_all(&self, path: &Path) -> RS<()>;
    async fn metadata_len(&self, path: &Path) -> RS<u64>;
    async fn path_exists(&self, path: &Path) -> RS<bool>;
    async fn remove_file_if_exists(&self, path: &Path) -> RS<()>;
    async fn read_dir(&self, path: &Path) -> RS<Vec<PathBuf>>;

    async fn read_all(&self, path: &Path) -> RS<Vec<u8>> {
        let file = self.open(path, FileOptions::read_only()).await?;
        let len = file.file_len().await?;
        file.read_exact_at(0, len as usize).await
    }

    async fn remove_dir_all(&self, _path: &Path) -> RS<()> {
        Err(mudu::m_error!(
            mudu::error::ec::EC::NotImplemented,
            "remove_dir_all is not implemented"
        ))
    }

    async fn write_all(&self, _path: &Path, _data: &[u8]) -> RS<()> {
        Err(mudu::m_error!(
            mudu::error::ec::EC::NotImplemented,
            "write_all is not implemented"
        ))
    }

    async fn read_to_string(&self, path: &Path) -> RS<String> {
        let bytes = self.read_all(path).await?;
        String::from_utf8(bytes)
            .map_err(|e| mudu::m_error!(mudu::error::ec::EC::IOErr, "invalid utf8", e))
    }
}
