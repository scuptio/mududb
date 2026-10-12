/// Raw file-descriptor type alias.
pub mod fd;
/// Socket address and raw-fd socket option helpers.
pub mod net;
/// Shared operation state used to complete futures from IO completions.
pub mod user_io;
/// Thread-local worker IO ring access.
pub mod worker_ring;

/// Linux-specific io_uring IO primitives.
#[cfg(target_os = "linux")]
pub mod linux {
    pub use crate::imp::native::linux::io_uring::{iouring, path, socket, worker_ring};
}
