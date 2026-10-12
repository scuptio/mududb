use super::metadata::SMetadata;
use mudu::common::result::RS;
use mudu::error::others::io_error_with_message;
use std::fmt;
#[cfg(unix)]
use std::os::fd::{AsRawFd, RawFd};
use std::path::Path;

/// Synchronous file handle with cursor-based IO ([`Read`](std::io::Read),
/// [`Write`](std::io::Write), [`Seek`](std::io::Seek)).
pub struct SFile {
    inner: std::fs::File,
}

impl SFile {
    /// Opens the file at `path` read-only using the default IO context.
    pub fn open(path: impl AsRef<Path>) -> RS<Self> {
        crate::default_sys_io_context()
            .fs_sync()
            .open(path.as_ref())
    }

    /// Creates (or truncates) the file at `path` using the default IO
    /// context.
    pub fn create(path: impl AsRef<Path>) -> RS<Self> {
        crate::default_sys_io_context()
            .fs_sync()
            .create(path.as_ref())
    }

    pub(crate) fn from_inner(inner: std::fs::File) -> Self {
        Self { inner }
    }

    /// Returns the metadata of this file.
    pub fn metadata(&self) -> RS<SMetadata> {
        self.inner
            .metadata()
            .map(SMetadata::from_inner)
            .map_err(|e| io_error_with_message(e, "file metadata error"))
    }

    /// Flushes file contents and metadata to stable storage.
    pub fn sync_all(&self) -> RS<()> {
        self.inner
            .sync_all()
            .map_err(|e| io_error_with_message(e, "file sync_all error"))
    }

    /// Flushes file contents (without requiring metadata) to stable storage.
    pub fn sync_data(&self) -> RS<()> {
        self.inner
            .sync_data()
            .map_err(|e| io_error_with_message(e, "file sync_data error"))
    }

    /// Truncates or extends the file to `size` bytes.
    pub fn set_len(&self, size: u64) -> RS<()> {
        self.inner
            .set_len(size)
            .map_err(|e| io_error_with_message(e, "file set_len error"))
    }

    /// Creates a new handle referring to the same open file description.
    pub fn try_clone(&self) -> RS<Self> {
        self.inner
            .try_clone()
            .map(Self::from_inner)
            .map_err(|e| io_error_with_message(e, "file try_clone error"))
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
