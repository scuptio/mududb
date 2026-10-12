use mudu::common::result::RS;
use mudu::error::others::io_error_with_message;
#[cfg(unix)]
use mudu::error::ErrorCode;
#[cfg(unix)]
use mudu::mudu_error;
#[cfg(unix)]
use std::os::fd::{AsRawFd, RawFd};
use std::sync::Arc;

/// Positioned (offset-based) synchronous file IO.
///
/// All operations address absolute byte offsets and do not disturb any
/// shared file cursor, so a single handle can be used concurrently.
pub trait SyncFile: Send + Sync {
    /// Reads exactly `len` bytes starting at absolute byte `offset`.
    fn read_exact_at(&self, offset: u64, len: usize) -> RS<Vec<u8>>;
    /// Writes all of `payload` starting at absolute byte `offset`.
    fn write_all_at(&self, offset: u64, payload: &[u8]) -> RS<()>;
    /// Flushes file contents and metadata to stable storage.
    fn fsync(&self) -> RS<()>;
    /// Returns the current length of the file in bytes.
    fn file_len(&self) -> RS<u64>;
    /// Closes the file. For the std-backed implementation this is a no-op:
    /// the file is closed when the handle is dropped.
    fn close(&self) -> RS<()>;

    /// Returns the underlying raw file descriptor, if the backend has one
    /// (Unix only).
    #[cfg(unix)]
    fn as_raw_fd(&self) -> Option<RawFd>;
}

/// Shared handle to a file accessed through [`SyncFile`] positioned IO.
#[derive(Clone)]
pub struct SyncSysFile {
    inner: Arc<dyn SyncFile>,
}

impl SyncSysFile {
    /// Wraps a [`SyncFile`] implementation in a shared handle.
    pub fn new(inner: Arc<dyn SyncFile>) -> Self {
        Self { inner }
    }

    /// Wraps a [`std::fs::File`] in a shared positioned-IO handle.
    pub fn from_std(file: std::fs::File) -> Self {
        Self::new(Arc::new(StdSyncFile::new(file)))
    }

    /// Reads exactly `len` bytes starting at absolute byte `offset`.
    pub fn read_exact_at(&self, offset: u64, len: usize) -> RS<Vec<u8>> {
        self.inner.read_exact_at(offset, len)
    }

    /// Writes all of `payload` starting at absolute byte `offset`.
    pub fn write_all_at(&self, offset: u64, payload: &[u8]) -> RS<()> {
        self.inner.write_all_at(offset, payload)
    }

    /// Flushes file contents and metadata to stable storage.
    pub fn fsync(&self) -> RS<()> {
        self.inner.fsync()
    }

    /// Returns the current length of the file in bytes.
    pub fn file_len(&self) -> RS<u64> {
        self.inner.file_len()
    }

    /// Closes the file.
    pub fn close(&self) -> RS<()> {
        self.inner.close()
    }

    /// Returns the underlying raw file descriptor, if the backend has one
    /// (Unix only).
    #[cfg(unix)]
    pub fn as_raw_fd(&self) -> Option<RawFd> {
        self.inner.as_raw_fd()
    }
}

pub(crate) struct StdSyncFile {
    inner: std::fs::File,
}

impl StdSyncFile {
    pub fn new(inner: std::fs::File) -> Self {
        Self { inner }
    }
}

impl SyncFile for StdSyncFile {
    fn read_exact_at(&self, offset: u64, len: usize) -> RS<Vec<u8>> {
        let mut buf = vec![0; len];

        #[cfg(unix)]
        {
            use std::os::unix::fs::FileExt;

            self.inner
                .read_exact_at(&mut buf, offset)
                .map_err(|e| io_error_with_message(e, "file read_exact_at error"))?;
        }

        #[cfg(not(unix))]
        {
            use std::io::{Read, Seek, SeekFrom};

            let mut file = self
                .inner
                .try_clone()
                .map_err(|e| io_error_with_message(e, "file try_clone error"))?;
            file.seek(SeekFrom::Start(offset))
                .map_err(|e| io_error_with_message(e, "file seek error"))?;
            file.read_exact(&mut buf)
                .map_err(|e| io_error_with_message(e, "file read_exact error"))?;
        }

        Ok(buf)
    }

    fn write_all_at(&self, offset: u64, payload: &[u8]) -> RS<()> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::FileExt;

            let mut written = 0;
            while written < payload.len() {
                let bytes = self
                    .inner
                    .write_at(&payload[written..], offset + written as u64)
                    .map_err(|e| io_error_with_message(e, "file write_at error"))?;
                if bytes == 0 {
                    return Err(mudu_error!(
                        ErrorCode::WriteZero,
                        "file write_at wrote zero bytes"
                    ));
                }
                written += bytes;
            }
        }

        #[cfg(not(unix))]
        {
            use std::io::{Seek, SeekFrom, Write};

            let mut file = self
                .inner
                .try_clone()
                .map_err(|e| io_error_with_message(e, "file try_clone error"))?;
            file.seek(SeekFrom::Start(offset))
                .map_err(|e| io_error_with_message(e, "file seek error"))?;
            file.write_all(payload)
                .map_err(|e| io_error_with_message(e, "file write_all error"))?;
        }

        Ok(())
    }

    fn fsync(&self) -> RS<()> {
        self.inner
            .sync_all()
            .map_err(|e| io_error_with_message(e, "file fsync error"))
    }

    fn file_len(&self) -> RS<u64> {
        self.inner
            .metadata()
            .map(|metadata| metadata.len())
            .map_err(|e| io_error_with_message(e, "file metadata error"))
    }

    fn close(&self) -> RS<()> {
        Ok(())
    }

    #[cfg(unix)]
    fn as_raw_fd(&self) -> Option<RawFd> {
        Some(self.inner.as_raw_fd())
    }
}
