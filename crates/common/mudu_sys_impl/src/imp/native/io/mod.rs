#![allow(missing_docs)]
pub mod fd;
pub mod net;
pub mod user_io;
pub mod worker_ring;

#[cfg(target_os = "linux")]
pub mod linux {
    pub use crate::imp::native::linux::io_uring::{iouring, path, socket, worker_ring};
}
