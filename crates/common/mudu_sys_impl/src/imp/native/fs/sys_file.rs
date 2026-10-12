use crate::contract::async_file::AsyncFile;
use mudu::common::result::RS;
#[cfg(unix)]
use std::os::fd::RawFd;
use std::sync::Arc;

/// Shared handle to a file accessed through an async [`AsyncFile`] backend
/// with positioned (offset-based) IO.
#[derive(Clone)]
pub struct SysFile {
    inner: Arc<dyn AsyncFile>,
}

impl SysFile {
    /// Wraps an [`AsyncFile`] implementation in a shared handle.
    pub fn new(inner: Arc<dyn AsyncFile>) -> Self {
        Self { inner }
    }

    /// Reads exactly `len` bytes starting at absolute byte `offset`.
    pub async fn read_exact_at(&self, offset: u64, len: usize) -> RS<Vec<u8>> {
        self.inner.read_exact_at(offset, len).await
    }
    /// Writes all of `payload` starting at absolute byte `offset`.
    pub async fn write_all_at(&self, offset: u64, payload: &[u8]) -> RS<()> {
        self.inner.write_all_at(offset, payload).await
    }

    /// Flushes file contents and metadata to stable storage.
    pub async fn fsync(&self) -> RS<()> {
        self.inner.fsync().await
    }

    /// Returns the current length of the file in bytes.
    pub async fn file_len(&self) -> RS<u64> {
        self.inner.file_len().await
    }

    /// Closes the file.
    pub async fn close(&self) -> RS<()> {
        self.inner.close().await
    }

    /// Returns the underlying raw file descriptor, if the backend has one
    /// (Unix only).
    #[cfg(unix)]
    pub fn as_raw_fd(&self) -> Option<RawFd> {
        self.inner.as_raw_fd()
    }
}
