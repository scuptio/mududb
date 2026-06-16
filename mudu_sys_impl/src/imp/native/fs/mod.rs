use crate::common::std_file::StdAsyncFile;
use crate::contract::async_file::AsyncFile;
use crate::contract::file_options::FileOptions;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub(crate) mod async_;
pub mod async__;
pub(crate) mod async_io_uring;
pub(crate) mod async_tokio;
pub mod sync;

pub struct Fs;

impl Default for Fs {
    fn default() -> Self {
        Self::new()
    }
}

impl Fs {
    pub fn new() -> Self {
        Self
    }

    pub async fn open(&self, path: &Path, options: FileOptions) -> RS<Arc<File>> {
        let mut open = std::fs::OpenOptions::new();
        open.read(options.read);
        open.write(options.write || options.append);
        open.create(options.create);
        open.truncate(options.truncate);
        open.append(options.append);
        open.create_new(options.create_new);
        let file = open
            .open(path)
            .map_err(|e| m_error!(EC::IOErr, "open file error", e))?;
        Ok(Arc::new(File::new(file)))
    }

    pub async fn create_dir_all(&self, path: &Path) -> RS<()> {
        std::fs::create_dir_all(path).map_err(|e| m_error!(EC::IOErr, "create directory error", e))
    }

    pub async fn metadata_len(&self, path: &Path) -> RS<u64> {
        std::fs::metadata(path)
            .map_err(|e| m_error!(EC::IOErr, "read file metadata error", e))
            .map(|metadata| metadata.len())
    }

    pub async fn path_exists(&self, path: &Path) -> RS<bool> {
        Ok(std::fs::metadata(path).is_ok())
    }

    pub async fn remove_file_if_exists(&self, path: &Path) -> RS<()> {
        match std::fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(err) => Err(m_error!(EC::IOErr, "remove file error", err)),
        }
    }

    pub async fn read_dir(&self, path: &Path) -> RS<Vec<PathBuf>> {
        let mut paths = Vec::new();
        for entry in
            std::fs::read_dir(path).map_err(|e| m_error!(EC::IOErr, "read directory error", e))?
        {
            let entry = entry.map_err(|e| m_error!(EC::IOErr, "read directory entry error", e))?;
            paths.push(entry.path());
        }
        Ok(paths)
    }

    pub async fn remove_dir_all(&self, path: &Path) -> RS<()> {
        std::fs::remove_dir_all(path).map_err(|e| m_error!(EC::IOErr, "remove directory error", e))
    }

    pub async fn write_all(&self, path: &Path, data: &[u8]) -> RS<()> {
        std::fs::write(path, data).map_err(|e| m_error!(EC::IOErr, "write file error", e))
    }

    pub async fn read_all(&self, path: &Path) -> RS<Vec<u8>> {
        std::fs::read(path).map_err(|e| m_error!(EC::IOErr, "read file error", e))
    }

    pub async fn read_to_string(&self, path: &Path) -> RS<String> {
        std::fs::read_to_string(path)
            .map_err(|e| m_error!(EC::IOErr, "read file to string error", e))
    }
}

pub struct File {
    inner: StdAsyncFile,
}

impl File {
    pub fn new(file: std::fs::File) -> Self {
        Self {
            inner: StdAsyncFile::new(file),
        }
    }

    pub async fn read_exact_at(&self, offset: u64, len: usize) -> RS<Vec<u8>> {
        self.inner.read_exact_at(offset, len).await
    }

    pub async fn write_all_at(&self, offset: u64, payload: &[u8]) -> RS<()> {
        self.inner.write_all_at(offset, payload).await
    }

    pub async fn fsync(&self) -> RS<()> {
        self.inner.fsync().await
    }

    pub async fn file_len(&self) -> RS<u64> {
        self.inner.file_len().await
    }

    pub async fn close(&self) -> RS<()> {
        self.inner.close().await
    }
}

#[async_trait::async_trait]
impl AsyncFile for File {
    async fn read_exact_at(&self, offset: u64, len: usize) -> RS<Vec<u8>> {
        File::read_exact_at(self, offset, len).await
    }

    async fn write_all_at(&self, offset: u64, payload: &[u8]) -> RS<()> {
        File::write_all_at(self, offset, payload).await
    }

    async fn fsync(&self) -> RS<()> {
        File::fsync(self).await
    }

    async fn file_len(&self) -> RS<u64> {
        File::file_len(self).await
    }

    async fn close(&self) -> RS<()> {
        File::close(self).await
    }
}
