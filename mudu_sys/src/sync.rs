//! Compatibility facade for synchronization helpers.
//!
//! New code should import async notification primitives from
//! [`crate::sync_async`] and OS/blocking helpers from [`crate::sync_sync`].
#[cfg(not(target_arch = "wasm32"))]
pub mod a_mutex;
#[cfg(not(target_arch = "wasm32"))]
pub mod a_notify;
#[cfg(not(target_arch = "wasm32"))]
pub mod a_rwlock;
pub mod a_task;
pub mod async_task;
pub mod f_mutex;
#[cfg(not(target_arch = "wasm32"))]
pub mod notify_wait;
pub mod s_mutex;
pub mod s_task;
#[cfg(not(target_arch = "wasm32"))]
pub mod stop_flag;
pub mod unique_inner;
pub use crate::sync::s_mutex::{SMutex, SMutexGuard};

#[cfg(not(target_arch = "wasm32"))]
pub use crate::sync_async::*;
pub use crate::sync_sync::*;
