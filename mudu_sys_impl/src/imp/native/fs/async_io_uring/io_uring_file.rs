use crate::imp::native::linux::io_uring::file::{self, FlushHandle, OptionWrite, WriteHandle};
use std::ffi::CString;

use crate::scoped_task_trace;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use std::os::fd::RawFd;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::contract::file_options::FileOptions;
use crate::io::worker_ring::has_current_worker_ring;
use tracing::trace;

pub struct IoUringFile {
    fd: RawFd,
    closed: AtomicBool,
}

impl Drop for IoUringFile {
    fn drop(&mut self) {
        if self.closed.swap(true, Ordering::Relaxed) {
            return;
        }
        unsafe {
            let _ = libc::close(self.fd);
        }
    }
}

impl IoUringFile {
    fn new_from_fd(fd: RawFd) -> Self {
        Self {
            fd,
            closed: AtomicBool::new(false),
        }
    }

    pub(crate) async fn open(path: impl AsRef<Path>, options: FileOptions) -> RS<Self> {
        scoped_task_trace!();
        trace!(path = %path.as_ref().display(), create = options.create, truncate = options.truncate, append = options.append, "iouring_fs open start");
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
        let file = iou_open(path, flags, 0o644).await?;
        Ok(file)
    }

    pub(crate) async fn read_exact_at(&self, offset: u64, len: usize) -> RS<Vec<u8>> {
        iou_read(self, len, offset).await
    }

    pub(crate) async fn write_all_at(&self, offset: u64, payload: &[u8]) -> RS<()> {
        let _n = iou_write(self, payload.to_vec(), offset).await?;
        Ok(())
    }

    pub(crate) async fn fsync(&self) -> RS<()> {
        iou_flush(self).await
    }

    pub(crate) async fn file_len(&self) -> RS<u64> {
        iou_file_len(self).await
    }

    #[allow(unused)]
    pub(crate) fn as_raw_fd(&self) -> Option<RawFd> {
        Some(self.fd)
    }
}

pub async fn iou_open<P: AsRef<Path>>(path: P, flags: i32, mode: u32) -> RS<IoUringFile> {
    scoped_task_trace!();
    #[cfg(target_os = "linux")]
    if has_current_worker_ring() {
        let _path_buf = path.as_ref().to_path_buf();
        let path = CString::new(path.as_ref().as_os_str().as_encoded_bytes())
            .map_err(|_| m_error!(EC::ParseErr, "path contains NUL byte"))?;
        let fd = file::FileOpenFuture::new(path, flags, mode).await?;
        Ok(IoUringFile::new_from_fd(fd))
    } else {
        panic!("do not support io_uring operation iou_open")
    }
}

pub async fn iou_read(file: &IoUringFile, len: usize, offset: u64) -> RS<Vec<u8>> {
    #[cfg(target_os = "linux")]
    if has_current_worker_ring() {
        file::FileReadFuture::new(file.fd, len, offset).await
    } else {
        panic!("do not support io_uring operation iou_read")
    }
}

pub async fn iou_write(file: &IoUringFile, data: Vec<u8>, offset: u64) -> RS<usize> {
    #[cfg(target_os = "linux")]
    if has_current_worker_ring() {
        iou_write_submit_option(file, data, offset, OptionWrite::default())?
            .wait()
            .await
    } else {
        panic!("do not support io_uring operation iou_write")
    }
}

pub async fn iou_file_len(file: &IoUringFile) -> RS<u64> {
    #[cfg(target_os = "linux")]
    if has_current_worker_ring() {
        file::FileLenFuture::new(file.fd).await
    } else {
        panic!("do not support io_uring operation iou_file_len")
    }
}

pub fn iou_write_submit_option(
    file: &IoUringFile,
    data: Vec<u8>,
    offset: u64,
    option: OptionWrite,
) -> RS<WriteHandle> {
    #[cfg(target_os = "linux")]
    if has_current_worker_ring() {
        return _iou_write_submit_option(file, data, offset, option);
    }

    let _ = (file, data, offset, option);
    Err(m_error!(
        EC::NotImplemented,
        "file write submit requires a worker ring; use async write outside io_uring workers"
    ))
}

#[cfg(target_os = "linux")]
fn _iou_write_submit_option(
    file: &IoUringFile,
    data: Vec<u8>,
    offset: u64,
    option: OptionWrite,
) -> RS<WriteHandle> {
    file::write_submit_option_fd(file.fd, data, offset, option)
}

pub async fn iou_flush(file: &IoUringFile) -> RS<()> {
    #[cfg(target_os = "linux")]
    if has_current_worker_ring() {
        iou_flush_submit(file)?.wait().await
    } else {
        panic!("do not support io_uring operation iou_flush")
    }
}

pub fn iou_flush_submit(file: &IoUringFile) -> RS<FlushHandle<()>> {
    iou_flush_submit_payload(file, ())
}

fn iou_flush_submit_payload<P>(file: &IoUringFile, payload: P) -> RS<FlushHandle<P>>
where
    P: Send + 'static,
{
    #[cfg(target_os = "linux")]
    if has_current_worker_ring() {
        return iou_flush_submit_payload_iouring(file, payload);
    }

    let _ = (file, payload);
    Err(m_error!(
        EC::NotImplemented,
        "file flush submit requires a worker ring; use async flush outside io_uring workers"
    ))
}

#[cfg(target_os = "linux")]
fn iou_flush_submit_payload_iouring<P>(file: &IoUringFile, payload: P) -> RS<FlushHandle<P>>
where
    P: Send + 'static,
{
    file::flush_submit_payload_fd(file.fd, payload)
}
