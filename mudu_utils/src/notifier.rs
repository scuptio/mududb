//! Async task-notification facade.
//!
//! This module intentionally remains separate from `task_sync`/`task_async`
//! because it only exposes async cancellation/notification primitives.
#[cfg(not(target_arch = "wasm32"))]
pub use mudu_sys::sync::async_::{Notifier, NotifyWait, Waiter, notify_wait};
