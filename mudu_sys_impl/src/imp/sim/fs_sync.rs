use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use std::path::{Path, PathBuf};

pub struct File;
pub struct Metadata;
pub struct DirEntry;
pub trait SyncFile: Send + Sync {}
#[derive(Clone)]
pub struct SyncSysFile;

impl File {
    pub fn into_inner(self) -> std::fs::File {
        panic!("[sim] FsSync::File has no native inner file")
    }
}

impl Metadata {
    pub fn into_inner(self) -> std::fs::Metadata {
        panic!("[sim] FsSync::Metadata has no native inner metadata")
    }
}

impl DirEntry {
    pub fn into_inner(self) -> std::fs::DirEntry {
        panic!("[sim] FsSync::DirEntry has no native inner dir entry")
    }
}

/// Synchronous filesystem operations — sim implementation (not implemented).
#[derive(Default)]
pub struct FsSync;

impl FsSync {
    pub fn new() -> Self {
        Self
    }

    pub fn create_dir_all(&self, _path: &Path) -> RS<()> {
        Err(m_error!(EC::NotImplemented, "[sim] FsSync::create_dir_all"))
    }

    pub fn remove_file(&self, _path: &Path) -> RS<()> {
        Err(m_error!(EC::NotImplemented, "[sim] FsSync::remove_file"))
    }

    pub fn remove_dir_all(&self, _path: &Path) -> RS<()> {
        Err(m_error!(EC::NotImplemented, "[sim] FsSync::remove_dir_all"))
    }

    pub fn read_all(&self, _path: &Path) -> RS<Vec<u8>> {
        Err(m_error!(EC::NotImplemented, "[sim] FsSync::read_all"))
    }

    pub fn write(&self, _path: &Path, _data: &[u8]) -> RS<()> {
        Err(m_error!(EC::NotImplemented, "[sim] FsSync::write"))
    }

    pub fn read_to_string(&self, _path: &Path) -> RS<String> {
        Err(m_error!(EC::NotImplemented, "[sim] FsSync::read_to_string"))
    }

    pub fn metadata_len(&self, _path: &Path) -> RS<u64> {
        Err(m_error!(EC::NotImplemented, "[sim] FsSync::metadata_len"))
    }

    pub fn path_exists(&self, _path: &Path) -> bool {
        false
    }

    pub fn read_dir(&self, _path: &Path) -> RS<Vec<PathBuf>> {
        Err(m_error!(EC::NotImplemented, "[sim] FsSync::read_dir"))
    }

    pub fn copy(&self, _from: &Path, _to: &Path) -> RS<u64> {
        Err(m_error!(EC::NotImplemented, "[sim] FsSync::copy"))
    }

    pub fn metadata(&self, _path: &Path) -> RS<Metadata> {
        Err(m_error!(EC::NotImplemented, "[sim] FsSync::metadata"))
    }

    pub fn read_dir_entries(&self, _path: &Path) -> RS<Vec<DirEntry>> {
        Err(m_error!(
            EC::NotImplemented,
            "[sim] FsSync::read_dir_entries"
        ))
    }

    pub fn open(&self, _path: &Path) -> RS<File> {
        Err(m_error!(EC::NotImplemented, "[sim] FsSync::open"))
    }

    pub fn open_sys_file(&self, _path: &Path) -> RS<SyncSysFile> {
        Err(m_error!(EC::NotImplemented, "[sim] FsSync::open_sys_file"))
    }

    pub fn create(&self, _path: &Path) -> RS<File> {
        Err(m_error!(EC::NotImplemented, "[sim] FsSync::create"))
    }

    pub fn open_with_options(&self, _path: &Path, _options: &std::fs::OpenOptions) -> RS<File> {
        Err(m_error!(
            EC::NotImplemented,
            "[sim] FsSync::open_with_options"
        ))
    }

    pub fn open_sys_file_with_options(
        &self,
        _path: &Path,
        _options: &std::fs::OpenOptions,
    ) -> RS<SyncSysFile> {
        Err(m_error!(
            EC::NotImplemented,
            "[sim] FsSync::open_sys_file_with_options"
        ))
    }
}
