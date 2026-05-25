//! Compatibility facade for task helpers.
//!
//! New code should import from [`crate::task_async`] or [`crate::task_sync`]
//! directly so sync and async call sites stay explicit.
#[cfg(not(target_arch = "wasm32"))]
pub use crate::task_async::*;
pub use crate::task_sync::*;
