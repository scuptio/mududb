use crate::imp::env::Sys;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use std::ffi::OsString;
use std::fmt;
#[cfg(unix)]
use std::os::fd::{AsRawFd, RawFd};
use std::path::{Path, PathBuf};

pub use crate::imp::fs_sync::{SyncFile, SyncSysFile};

pub fn sync_create_dir_all(path: impl AsRef<Path>) -> RS<()> {
    Sys::fs_sync().create_dir_all(path.as_ref())
}

pub fn sync_remove_file(path: impl AsRef<Path>) -> RS<()> {
    Sys::fs_sync().remove_file(path.as_ref())
}

pub fn sync_remove_dir_all(path: impl AsRef<Path>) -> RS<()> {
    Sys::fs_sync().remove_dir_all(path.as_ref())
}

pub fn sync_read_all(path: impl AsRef<Path>) -> RS<Vec<u8>> {
    Sys::fs_sync().read_all(path.as_ref())
}

pub fn sync_write(path: impl AsRef<Path>, data: impl AsRef<[u8]>) -> RS<()> {
    Sys::fs_sync().write(path.as_ref(), data.as_ref())
}

pub fn sync_read_to_string(path: impl AsRef<Path>) -> RS<String> {
    Sys::fs_sync().read_to_string(path.as_ref())
}

pub fn sync_path_exists(path: impl AsRef<Path>) -> bool {
    Sys::fs_sync().path_exists(path.as_ref())
}

pub fn sync_read_dir(path: impl AsRef<Path>) -> RS<Vec<PathBuf>> {
    Sys::fs_sync().read_dir(path.as_ref())
}

pub fn create_dir_all(path: impl AsRef<Path>) -> RS<()> {
    sync_create_dir_all(path)
}

pub fn remove_file(path: impl AsRef<Path>) -> RS<()> {
    sync_remove_file(path)
}

pub fn remove_dir_all(path: impl AsRef<Path>) -> RS<()> {
    sync_remove_dir_all(path)
}

pub fn read(path: impl AsRef<Path>) -> RS<Vec<u8>> {
    sync_read_all(path)
}

pub fn write(path: impl AsRef<Path>, data: impl AsRef<[u8]>) -> RS<()> {
    sync_write(path, data)
}

pub fn read_to_string(path: impl AsRef<Path>) -> RS<String> {
    sync_read_to_string(path)
}

pub fn path_exists(path: impl AsRef<Path>) -> bool {
    sync_path_exists(path)
}

pub fn read_dir(path: impl AsRef<Path>) -> RS<Vec<PathBuf>> {
    sync_read_dir(path)
}

pub fn open(path: impl AsRef<Path>) -> RS<SyncSysFile> {
    crate::imp::fs::sync::open(path.as_ref())
}

// ---------------------------------------------------------------------------
// SFile – synchronous file handle wrapper (no direct std::fs::File exposure)
// ---------------------------------------------------------------------------

pub struct SFile {
    inner: std::fs::File,
}

impl SFile {
    pub fn open(path: impl AsRef<Path>) -> RS<Self> {
        Sys::fs_sync()
            .open(path.as_ref())
            .map(|file| Self::from_inner(file.into_inner()))
    }

    pub fn create(path: impl AsRef<Path>) -> RS<Self> {
        Sys::fs_sync()
            .create(path.as_ref())
            .map(|file| Self::from_inner(file.into_inner()))
    }

    pub(crate) fn from_inner(inner: std::fs::File) -> Self {
        Self { inner }
    }

    pub fn metadata(&self) -> RS<SMetadata> {
        self.inner
            .metadata()
            .map(SMetadata::from_inner)
            .map_err(|e| m_error!(EC::IOErr, "file metadata error", e))
    }

    pub fn sync_all(&self) -> RS<()> {
        self.inner
            .sync_all()
            .map_err(|e| m_error!(EC::IOErr, "file sync_all error", e))
    }

    pub fn sync_data(&self) -> RS<()> {
        self.inner
            .sync_data()
            .map_err(|e| m_error!(EC::IOErr, "file sync_data error", e))
    }

    pub fn set_len(&self, size: u64) -> RS<()> {
        self.inner
            .set_len(size)
            .map_err(|e| m_error!(EC::IOErr, "file set_len error", e))
    }

    pub fn try_clone(&self) -> RS<Self> {
        self.inner
            .try_clone()
            .map(Self::from_inner)
            .map_err(|e| m_error!(EC::IOErr, "file try_clone error", e))
    }
}

impl std::io::Read for SFile {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.inner.read(buf)
    }
}

impl std::io::Write for SFile {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.inner.write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

impl std::io::Seek for SFile {
    fn seek(&mut self, pos: std::io::SeekFrom) -> std::io::Result<u64> {
        self.inner.seek(pos)
    }
}

#[cfg(unix)]
impl AsRawFd for SFile {
    fn as_raw_fd(&self) -> RawFd {
        self.inner.as_raw_fd()
    }
}

impl fmt::Debug for SFile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.inner.fmt(f)
    }
}

// ---------------------------------------------------------------------------
// SOpenOptions – synchronous file open options wrapper
// ---------------------------------------------------------------------------

pub struct SOpenOptions {
    inner: std::fs::OpenOptions,
}

impl SOpenOptions {
    pub fn new() -> Self {
        Self {
            inner: std::fs::OpenOptions::new(),
        }
    }

    pub fn read(&mut self, read: bool) -> &mut Self {
        self.inner.read(read);
        self
    }

    pub fn write(&mut self, write: bool) -> &mut Self {
        self.inner.write(write);
        self
    }

    pub fn append(&mut self, append: bool) -> &mut Self {
        self.inner.append(append);
        self
    }

    pub fn truncate(&mut self, truncate: bool) -> &mut Self {
        self.inner.truncate(truncate);
        self
    }

    pub fn create(&mut self, create: bool) -> &mut Self {
        self.inner.create(create);
        self
    }

    pub fn create_new(&mut self, create_new: bool) -> &mut Self {
        self.inner.create_new(create_new);
        self
    }

    pub fn open(&self, path: impl AsRef<Path>) -> RS<SFile> {
        Sys::fs_sync()
            .open_with_options(path.as_ref(), self.inner_ref())
            .map(|file| SFile::from_inner(file.into_inner()))
    }

    pub(crate) fn inner_ref(&self) -> &std::fs::OpenOptions {
        &self.inner
    }
}

impl Default for SOpenOptions {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// SMetadata – file metadata wrapper
// ---------------------------------------------------------------------------

pub struct SMetadata {
    inner: std::fs::Metadata,
}

impl SMetadata {
    pub(crate) fn from_inner(inner: std::fs::Metadata) -> Self {
        Self { inner }
    }

    pub fn len(&self) -> u64 {
        self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.len() == 0
    }

    pub fn is_file(&self) -> bool {
        self.inner.is_file()
    }

    pub fn is_dir(&self) -> bool {
        self.inner.is_dir()
    }

    pub fn is_symlink(&self) -> bool {
        self.inner.is_symlink()
    }
}

impl fmt::Debug for SMetadata {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.inner.fmt(f)
    }
}

// ---------------------------------------------------------------------------
// SDirEntry – directory entry wrapper
// ---------------------------------------------------------------------------

pub struct SDirEntry {
    inner: std::fs::DirEntry,
}

impl SDirEntry {
    pub(crate) fn from_inner(inner: std::fs::DirEntry) -> Self {
        Self { inner }
    }

    pub fn path(&self) -> PathBuf {
        self.inner.path()
    }

    pub fn file_name(&self) -> OsString {
        self.inner.file_name()
    }

    pub fn metadata(&self) -> RS<SMetadata> {
        self.inner
            .metadata()
            .map(|inner| SMetadata { inner })
            .map_err(|e| m_error!(EC::IOErr, "dir entry metadata error", e))
    }

    pub fn file_type(&self) -> RS<SFileType> {
        self.inner
            .file_type()
            .map(|inner| SFileType { inner })
            .map_err(|e| m_error!(EC::IOErr, "dir entry file_type error", e))
    }
}

// ---------------------------------------------------------------------------
// SFileType – file type wrapper
// ---------------------------------------------------------------------------

pub struct SFileType {
    inner: std::fs::FileType,
}

impl SFileType {
    pub fn is_file(&self) -> bool {
        self.inner.is_file()
    }

    pub fn is_dir(&self) -> bool {
        self.inner.is_dir()
    }

    pub fn is_symlink(&self) -> bool {
        self.inner.is_symlink()
    }
}

impl fmt::Debug for SFileType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.inner.fmt(f)
    }
}

// ---------------------------------------------------------------------------
// sync_read_dir_entries – returns a list of SDirEntry (alternative to sync_read_dir)
// ---------------------------------------------------------------------------

pub fn sync_read_dir_entries(path: impl AsRef<Path>) -> RS<Vec<SDirEntry>> {
    Sys::fs_sync()
        .read_dir_entries(path.as_ref())
        .map(|entries| {
            entries
                .into_iter()
                .map(|entry| SDirEntry::from_inner(entry.into_inner()))
                .collect()
        })
}

pub fn read_dir_entries(path: impl AsRef<Path>) -> RS<Vec<SDirEntry>> {
    sync_read_dir_entries(path)
}

// ---------------------------------------------------------------------------
// sync_copy / sync_metadata
// ---------------------------------------------------------------------------

pub fn sync_copy(from: impl AsRef<Path>, to: impl AsRef<Path>) -> RS<u64> {
    Sys::fs_sync().copy(from.as_ref(), to.as_ref())
}

pub fn copy(from: impl AsRef<Path>, to: impl AsRef<Path>) -> RS<u64> {
    sync_copy(from, to)
}

pub fn sync_metadata(path: impl AsRef<Path>) -> RS<SMetadata> {
    Sys::fs_sync()
        .metadata(path.as_ref())
        .map(|metadata| SMetadata::from_inner(metadata.into_inner()))
}

pub fn metadata(path: impl AsRef<Path>) -> RS<SMetadata> {
    sync_metadata(path)
}
