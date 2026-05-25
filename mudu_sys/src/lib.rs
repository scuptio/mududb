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
#[cfg(not(target_arch = "wasm32"))]
pub mod sync_async;
pub mod sync_sync;
#[deprecated(note = "use mudu_sys::task_async or mudu_sys::task_sync instead")]
pub mod task;
#[cfg(not(target_arch = "wasm32"))]
pub mod task_async;
pub mod task_context;
pub mod task_id;
pub mod task_sync;
#[cfg(target_os = "linux")]
#[path = "linux/uring.rs"]
pub mod uring;

#[cfg(not(target_arch = "wasm32"))]
pub use tokio;

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
