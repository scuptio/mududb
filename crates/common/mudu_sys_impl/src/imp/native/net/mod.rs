pub(crate) mod async_;
#[cfg(target_os = "linux")]
mod async_io_uring;
/// Tokio-based async TCP networking.
pub mod async_tokio;
/// Blocking TCP networking over `std::net` / `socket2`.
pub mod sync;
