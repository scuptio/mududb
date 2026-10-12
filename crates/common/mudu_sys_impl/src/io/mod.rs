//! IO helpers and public re-exports.
pub use crate::imp::io::fd;
pub use crate::imp::io::net;
pub use crate::imp::io::user_io;
pub use crate::imp::io::worker_ring;

/// io_uring path operations (Linux only).
#[cfg(target_os = "linux")]
pub mod path {
    pub use crate::imp::native::linux::io_uring::path::*;
}
/// Raw socket helpers built on io_uring (Linux only).
#[cfg(target_os = "linux")]
pub mod socket {
    pub use crate::imp::native::linux::io_uring::socket::*;
}
/// Low-level io_uring ring wrapper (Linux only).
#[cfg(target_os = "linux")]
pub mod iouring {
    pub use crate::imp::native::linux::io_uring::iouring::*;
}
