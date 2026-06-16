pub mod fd;
pub mod fs;
pub mod fs_sync;
pub mod net;
pub mod sys_file;
pub mod user_io;
pub mod worker_ring;

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
