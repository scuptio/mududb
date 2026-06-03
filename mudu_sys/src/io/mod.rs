pub mod file;
#[cfg(target_os = "linux")]
#[path = "linux/path.rs"]
pub mod path;
#[cfg(target_os = "linux")]
#[path = "linux/socket.rs"]
pub mod socket;
pub mod user_io;
#[cfg(target_os = "linux")]
#[path = "linux/worker_ring.rs"]
pub mod worker_ring;

#[cfg(target_os = "linux")]
#[path = "linux/iouring.rs"]
pub mod iouring;

#[cfg(not(target_os = "linux"))]
#[path = "portable/worker_ring.rs"]
pub mod worker_ring;
pub mod fd;
pub mod fs;
pub mod net;
pub mod sys_file;
