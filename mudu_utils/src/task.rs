//! Compatibility facade for task helpers.
//!
//! New code should import runtime/task functions from [`crate::task_async`] or
//! [`crate::task_sync`] directly.
pub use crate::task_async::*;
pub use crate::task_sync::*;
