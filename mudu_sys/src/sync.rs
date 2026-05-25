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
pub mod f_mutex;
#[cfg(not(target_arch = "wasm32"))]
pub mod notify_wait;
#[cfg(not(target_arch = "wasm32"))]
pub use crate::sync_async::*;
pub use crate::sync_sync::*;
