#![allow(missing_docs)]
pub mod async_;
pub mod sync;
pub mod sys_file;

pub(crate) mod async_io_uring;
pub(crate) mod async_tokio;

pub use sys_file::SysFile;

#[cfg(target_os = "linux")]
pub(crate) use async_io_uring::async_io_uring_fs::AsyncIoUringFs;
pub(crate) use async_tokio::async_tokio_fs::AsyncTokioFs;
