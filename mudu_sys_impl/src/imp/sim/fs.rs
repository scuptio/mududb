use crate::contract::async_file::AsyncFile;
use crate::contract::file_options::FileOptions;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub mod sync {
    pub use crate::imp::sim::fs_sync::{DirEntry, File, Metadata, SyncSysFile};
    use mudu::common::result::RS;
    use std::path::Path;

    pub fn open(path: impl AsRef<Path>) -> RS<SyncSysFile> {
        crate::imp::sim::env::Sys::fs_sync().open_sys_file(path.as_ref())
    }

    pub fn open_with_options(
        path: impl AsRef<Path>,
        options: &std::fs::OpenOptions,
    ) -> RS<SyncSysFile> {
        crate::imp::sim::env::Sys::fs_sync().open_sys_file_with_options(path.as_ref(), options)
    }
}

pub mod async_ {
    use crate::contract::file_options::FileOptions;
    pub use crate::io::sys_file::SysFile;
    use mudu::common::result::RS;
    use mudu::error::ec::EC;
    use mudu::m_error;
    use std::path::Path;

    pub async fn open(path: impl AsRef<Path>) -> RS<SysFile> {
        open_with_options(path, FileOptions::read_write_create()).await
    }

    pub async fn open_with_options(path: impl AsRef<Path>, options: FileOptions) -> RS<SysFile> {
        let file = crate::imp::sim::env::Sys::fs()
            .open(path.as_ref(), options)
            .await?;
        Ok(SysFile::new(file))
    }

    pub async fn open_raw(_path: impl AsRef<Path>, _flags: i32, _mode: u32) -> RS<SysFile> {
        Err(m_error!(EC::NotImplemented, "[sim] fs::async_::open_raw"))
    }
}

pub struct Fs;

impl Fs {
    pub async fn open(&self, _path: &Path, _options: FileOptions) -> RS<Arc<File>> {
        Err(m_error!(EC::NotImplemented, "[sim] Fs::open"))
    }

    pub async fn create_dir_all(&self, _path: &Path) -> RS<()> {
        Err(m_error!(EC::NotImplemented, "[sim] Fs::create_dir_all"))
    }

    pub async fn metadata_len(&self, _path: &Path) -> RS<u64> {
        Err(m_error!(EC::NotImplemented, "[sim] Fs::metadata_len"))
    }

    pub async fn path_exists(&self, _path: &Path) -> RS<bool> {
        Err(m_error!(EC::NotImplemented, "[sim] Fs::path_exists"))
    }

    pub async fn remove_file_if_exists(&self, _path: &Path) -> RS<()> {
        Err(m_error!(
            EC::NotImplemented,
            "[sim] Fs::remove_file_if_exists"
        ))
    }

    pub async fn read_dir(&self, _path: &Path) -> RS<Vec<PathBuf>> {
        Err(m_error!(EC::NotImplemented, "[sim] Fs::read_dir"))
    }

    pub async fn remove_dir_all(&self, _path: &Path) -> RS<()> {
        Err(m_error!(EC::NotImplemented, "[sim] Fs::remove_dir_all"))
    }

    pub async fn write_all(&self, _path: &Path, _data: &[u8]) -> RS<()> {
        Err(m_error!(EC::NotImplemented, "[sim] Fs::write_all"))
    }

    pub async fn read_all(&self, _path: &Path) -> RS<Vec<u8>> {
        Err(m_error!(EC::NotImplemented, "[sim] Fs::read_all"))
    }

    pub async fn read_to_string(&self, _path: &Path) -> RS<String> {
        Err(m_error!(EC::NotImplemented, "[sim] Fs::read_to_string"))
    }
}

pub struct File;

impl File {
    pub async fn read_exact_at(&self, _offset: u64, _len: usize) -> RS<Vec<u8>> {
        Err(m_error!(EC::NotImplemented, "[sim] File::read_exact_at"))
    }

    pub async fn write_all_at(&self, _offset: u64, _payload: &[u8]) -> RS<()> {
        Err(m_error!(EC::NotImplemented, "[sim] File::write_all_at"))
    }

    pub async fn fsync(&self) -> RS<()> {
        Err(m_error!(EC::NotImplemented, "[sim] File::fsync"))
    }

    pub async fn file_len(&self) -> RS<u64> {
        Err(m_error!(EC::NotImplemented, "[sim] File::file_len"))
    }

    pub async fn close(&self) -> RS<()> {
        Err(m_error!(EC::NotImplemented, "[sim] File::close"))
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
