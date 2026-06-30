//! IO helpers and public re-exports.
#![allow(missing_docs)]
pub use crate::imp::io::fd;
pub use crate::imp::io::net;
pub use crate::imp::io::user_io;
pub use crate::imp::io::worker_ring;

#[cfg(target_os = "linux")]
pub mod path {
    pub use crate::imp::native::linux::io_uring::path::*;
}
#[cfg(target_os = "linux")]
pub mod socket {
    pub use crate::imp::native::linux::io_uring::socket::*;
}
#[cfg(target_os = "linux")]
pub mod iouring {
    pub use crate::imp::native::linux::io_uring::iouring::*;
}
