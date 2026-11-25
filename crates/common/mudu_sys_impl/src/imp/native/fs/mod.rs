/// Async filesystem facade functions operating through the default system
/// environment's IO provider.
pub mod async_;
/// Synchronous filesystem operations.
pub mod sync;
/// Async file handle shared across IO providers.
pub mod sys_file;

#[cfg(target_os = "linux")]
pub(crate) mod async_io_uring;
pub(crate) mod async_tokio;

pub use sys_file::SysFile;

#[cfg(target_os = "linux")]
pub(crate) use async_io_uring::async_io_uring_fs::AsyncIoUringFs;
pub(crate) use async_tokio::async_tokio_fs::AsyncTokioFs;

/// Creates a Tokio-backed filesystem independent of the worker's IO provider.
/// File operations use Tokio's blocking pool and never block their caller's executor.
pub fn tokio_fs() -> std::sync::Arc<dyn crate::contract::async_fs::AsyncFs> {
    std::sync::Arc::new(AsyncTokioFs::new())
}
