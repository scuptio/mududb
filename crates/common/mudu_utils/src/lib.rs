//! I/O-performing utility facade for the MuduDB workspace.
//!
//! `mudu_utils` builds on [`mudu`] and [`mudu_sys`] and hosts the higher-level
//! helpers that are allowed to touch the filesystem, the network, the tracing
//! subscriber, and the task runtimes: JSON/TOML file helpers, logging setup,
//! task/thread trace instrumentation, and the debug HTTP server.

#![warn(missing_docs)]

/// Debug HTTP server exposing task traces and registered debug URLs.
pub mod debug;
#[cfg(test)]
mod debug_test;
#[cfg(not(target_arch = "wasm32"))]
mod init_log;
#[cfg(test)]
mod init_log_test;
/// JSON file read/write helpers and re-exports of the `mudu` JSON types.
pub mod json;
/// Process-wide tracing subscriber initialization.
#[cfg(not(target_arch = "wasm32"))]
pub mod log;
/// MD5 digest helper.
pub mod md5;
/// Async notification primitives re-exported from `mudu_sys`.
pub mod notifier;
/// Random 128-bit identifier generation.
pub mod oid;
pub use oid::{gen_oid, new_xid};
/// Synchronization primitives re-exported from `mudu_sys`.
pub mod sync;
/// Deprecated compatibility facade over [`task_async`] and [`task_sync`].
#[deprecated(note = "use mudu_utils::task_async or mudu_utils::task_sync instead")]
pub mod task;
/// Async task and runtime facade over `mudu_sys::task::async_`.
pub mod task_async;
/// Task context registry re-exported from `mudu_sys`.
pub mod task_context;
/// Task identifier types re-exported from `mudu_sys`.
pub mod task_id;
mod task_macros;
/// Blocking thread facade over `mudu_sys::task::sync`.
pub mod task_sync;
/// Async task trace instrumentation used by the `task_trace!` macros.
pub mod task_trace;
#[cfg(test)]
mod task_trace_test;
mod test_debug_server;
/// Cooperative yield for manually-polled execution contexts.
pub mod yield_now;

// Re-exported so the `task_trace!` macro can refer to it via `$crate`
// without forcing every caller to depend on `async-backtrace` directly.
pub use async_backtrace;
/// Location of the invoking source file, backing the `this_file!` macro.
pub mod this_file;
/// Thread trace instrumentation used by the `thread_trace!` macros.
pub mod thread_trace;
/// TOML serialization and file read/write helpers.
pub mod toml;

/// Returns the sum of `left` and `right`.
pub fn add(left: u64, right: u64) -> u64 {
    left + right
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let result = add(2, 2);
        assert_eq!(result, 4);
    }
}
