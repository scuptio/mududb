use crate::api::fs::SysFs;
use crate::async_rt::std_file::StdAsyncFile;
use crate::io::sys_file::SysFile;
use async_trait::async_trait;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub struct LinuxFs;

#[async_trait]
impl SysFs for LinuxFs {
    async fn open(&self, path: &Path, flags: i32, mode: u32) -> RS<SysFile> {
        let std_file = StdAsyncFile::open(path, flags, mode)
            .map_err(|e| m_error!(EC::IOErr, "open file error", e))?;
        Ok(SysFile::new(Arc::new(std_file)))
    }

    async fn read_exact_at(&self, file: &SysFile, len: usize, offset: u64) -> RS<Vec<u8>> {
        file.read_exact_at(offset, len).await
    }

    async fn write_all_at(&self, file: &SysFile, payload: &[u8], offset: u64) -> RS<()> {
        file.write_all_at(offset, payload).await
    }

    async fn fsync(&self, file: &SysFile) -> RS<()> {
        file.fsync().await
    }

    async fn close(&self, _file: SysFile) -> RS<()> {
        Ok(())
    }

    async fn create_dir_all(&self, path: &Path) -> RS<()> {
        std::fs::create_dir_all(path).map_err(|e| m_error!(EC::IOErr, "create directory error", e))
    }

    async fn read_dir(&self, path: &Path) -> RS<Vec<PathBuf>> {
        let mut paths = Vec::new();
        for entry in
            std::fs::read_dir(path).map_err(|e| m_error!(EC::IOErr, "read directory error", e))?
        {
            let entry = entry.map_err(|e| m_error!(EC::IOErr, "read directory entry error", e))?;
            paths.push(entry.path());
        }
        Ok(paths)
    }

    async fn metadata_len(&self, path: &Path) -> RS<u64> {
        std::fs::metadata(path)
            .map_err(|e| m_error!(EC::IOErr, "read file metadata error", e))
            .map(|metadata| metadata.len())
    }

    async fn read_all(&self, path: &Path) -> RS<Vec<u8>> {
        std::fs::read(path).map_err(|e| m_error!(EC::IOErr, "read file error", e))
    }

    async fn remove_file_if_exists(&self, path: &Path) -> RS<()> {
        match std::fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(err) => Err(m_error!(EC::IOErr, "remove file error", err)),
        }
    }
}
