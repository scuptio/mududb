pub mod api;
pub mod env;
pub mod fd;
pub mod fs;
#[cfg(target_os = "linux")]
pub mod linux;
pub mod net;
#[cfg(not(target_os = "linux"))]
mod portable;
pub mod sync;
pub mod task;
#[cfg(target_os = "linux")]
#[path = "linux/uring.rs"]
pub mod uring;

pub mod random {
    pub use crate::api::random::{next_uuid_v4_string, uuid_v4};
}

pub mod time {
    pub use crate::api::time::{instant_now, system_time_now, utc_now};
}

pub fn io_uring_available() -> bool {
    #[cfg(target_os = "linux")]
    {
        crate::uring::IoUring::new(8).is_ok()
    }
    #[cfg(not(target_os = "linux"))]
    {
        false
    }
}
