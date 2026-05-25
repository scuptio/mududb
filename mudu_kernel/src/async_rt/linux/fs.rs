use crate::async_rt::contract::{AsyncFile, AsyncFs, FileOpenOptions};
use crate::io::file::{self, IoFile};
use crate::io::path;
use async_trait::async_trait;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use mudu_utils::scoped_task_trace;
use mudu_utils::sync::a_mutex::AMutex;
use std::mem::ManuallyDrop;
use std::os::fd::RawFd;
use std::os::unix::io::FromRawFd;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tracing::trace;

#[derive(Default)]
pub struct IoUringFs;

impl IoUringFs {
    pub const fn new() -> Self {
        Self
    }
}

pub struct IoUringFile {
    fd: RawFd,
    closed: AtomicBool,
    inner: AMutex<IoFile>,
}

impl IoUringFile {
    fn new(file: IoFile) -> Self {
        Self {
            fd: file.fd(),
            closed: AtomicBool::new(false),
            inner: AMutex::new(file),
        }
    }
}

impl Drop for IoUringFile {
    fn drop(&mut self) {
        if self.closed.swap(true, Ordering::Relaxed) {
            return;
        }
        let _ = file::close_sync(IoFile::from_raw_fd(self.fd));
    }
}

#[async_trait]
impl AsyncFs for IoUringFs {
    async fn open(&self, path: &Path, options: FileOpenOptions) -> RS<Arc<dyn AsyncFile>> {
        scoped_task_trace!();
        trace!(path = %path.display(), create = options.create, truncate = options.truncate, append = options.append, "iouring_fs open start");
        let mut flags = 0;
        if options.read && options.write {
            flags |= libc::O_RDWR;
        } else if options.write {
            flags |= libc::O_WRONLY;
        } else {
            flags |= libc::O_RDONLY;
        }
        if options.create {
            flags |= libc::O_CREAT;
        }
        if options.truncate {
            flags |= libc::O_TRUNC;
        }
        if options.append {
            flags |= libc::O_APPEND;
        }
        if options.create_new {
            flags |= libc::O_EXCL | libc::O_CREAT;
        }
        let file = file::open(path, flags, 0o644).await?;
        Ok(Arc::new(IoUringFile::new(file)))
    }

    async fn create_dir_all(&self, path: &Path) -> RS<()> {
        path::create_dir_all(path).await
    }

    async fn metadata_len(&self, path: &Path) -> RS<u64> {
        path::metadata_len(path).await
    }

    async fn path_exists(&self, path: &Path) -> RS<bool> {
        path::path_exists(path).await
    }

    async fn remove_file_if_exists(&self, path: &Path) -> RS<()> {
        path::remove_file_if_exists(path).await
    }
}

#[async_trait]
impl AsyncFile for IoUringFile {
    async fn read_exact_at(&self, offset: u64, len: usize) -> RS<Vec<u8>> {
        let file = self.inner.lock().await;
        file::read(&file, len, offset).await
    }

    async fn write_all_at(&self, offset: u64, payload: &[u8]) -> RS<()> {
        let file = self.inner.lock().await;
        let written = file::write(&file, payload.to_vec(), offset).await?;
        if written != payload.len() {
            return Err(m_error!(
                EC::IOErr,
                format!(
                    "io_uring file write incomplete: wrote {}, expected {}",
                    written,
                    payload.len()
                )
            ));
        }
        Ok(())
    }

    async fn fsync(&self) -> RS<()> {
        let file = self.inner.lock().await;
        file::flush(&file).await
    }

    async fn file_len(&self) -> RS<u64> {
        let file = self.inner.lock().await;
        let std_file = unsafe { ManuallyDrop::new(std::fs::File::from_raw_fd(file.fd())) };
        std_file
            .metadata()
            .map(|metadata| metadata.len())
            .map_err(|e| m_error!(EC::IOErr, "read io_uring file metadata error", e))
    }
}
