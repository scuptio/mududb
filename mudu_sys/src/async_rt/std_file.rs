use crate::async_rt::contract::AsyncFile;
use crate::sync::s_mutex::SMutex;
use async_trait::async_trait;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
#[cfg(unix)]
use std::os::fd::{AsRawFd, RawFd};
#[cfg(unix)]
use std::os::unix::fs::{FileExt, OpenOptionsExt};
#[cfg(windows)]
use std::os::windows::fs::FileExt;
use std::path::Path;

pub struct StdAsyncFile {
    inner: SMutex<std::fs::File>,
}

impl StdAsyncFile {
    pub fn new(file: std::fs::File) -> Self {
        Self {
            inner: SMutex::new(file),
        }
    }

    pub fn open(path: &Path, flags: i32, _mode: u32) -> RS<Self> {
        let mut options = std::fs::OpenOptions::new();
        let read = (flags & libc::O_RDWR) != 0 || (flags & libc::O_WRONLY) == 0;
        let write = (flags & libc::O_RDWR) != 0 || (flags & libc::O_WRONLY) != 0;
        options.read(read);
        options.write(write);
        options.create((flags & libc::O_CREAT) != 0);
        options.truncate((flags & libc::O_TRUNC) != 0);
        options.append((flags & libc::O_APPEND) != 0);
        #[cfg(unix)]
        {
            options.mode(_mode);
        }
        let file = options
            .open(path)
            .map_err(|e| m_error!(EC::IOErr, "open file error", e))?;
        Ok(Self::new(file))
    }
}

#[cfg(unix)]
impl AsRawFd for StdAsyncFile {
    fn as_raw_fd(&self) -> RawFd {
        self.inner.lock().unwrap().as_raw_fd()
    }
}

#[async_trait]
impl AsyncFile for StdAsyncFile {
    async fn read_exact_at(&self, offset: u64, len: usize) -> RS<Vec<u8>> {
        let mut buf = vec![0u8; len];
        let mut read = 0usize;
        while read < len {
            let rc = self
                .inner
                .lock()
                .unwrap()
                .read_at(&mut buf[read..], offset + read as u64)
                .map_err(|e| m_error!(EC::IOErr, "read file error", e))?;
            if rc == 0 {
                return Err(m_error!(EC::IOErr, "unexpected EOF while reading file"));
            }
            read += rc;
        }
        Ok(buf)
    }

    async fn write_all_at(&self, offset: u64, payload: &[u8]) -> RS<()> {
        let mut written = 0usize;
        while written < payload.len() {
            let rc = self
                .inner
                .lock()
                .unwrap()
                .write_at(&payload[written..], offset + written as u64)
                .map_err(|e| m_error!(EC::IOErr, "write file error", e))?;
            if rc == 0 {
                return Err(m_error!(EC::IOErr, "write file returned zero bytes"));
            }
            written += rc;
        }
        Ok(())
    }

    async fn fsync(&self) -> RS<()> {
        self.inner
            .lock()
            .unwrap()
            .sync_all()
            .map_err(|e| m_error!(EC::IOErr, "flush file error", e))
    }

    async fn file_len(&self) -> RS<u64> {
        self.inner
            .lock()
            .unwrap()
            .metadata()
            .map_err(|e| m_error!(EC::IOErr, "read file metadata error", e))
            .map(|metadata| metadata.len())
    }

    fn as_raw_fd(&self) -> Option<RawFd> {
        #[cfg(unix)]
        {
            Some(std::os::fd::AsRawFd::as_raw_fd(self))
        }
        #[cfg(not(unix))]
        {
            None
        }
    }
}
