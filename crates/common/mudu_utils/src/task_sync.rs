//! Blocking/thread-based task facade used by higher-level crates.
pub use mudu_sys::task::sync::{sleep_blocking, spawn_thread, spawn_thread_named};
