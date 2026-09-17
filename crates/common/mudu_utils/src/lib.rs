pub mod debug;
#[cfg(test)]
mod debug_test;
#[cfg(not(target_arch = "wasm32"))]
mod init_log;
#[cfg(test)]
mod init_log_test;
pub mod json;
#[cfg(not(target_arch = "wasm32"))]
pub mod log;
pub mod md5;
pub mod notifier;
pub mod oid;
pub use oid::{gen_oid, new_xid};
pub mod sync;
#[deprecated(note = "use mudu_utils::task_async or mudu_utils::task_sync instead")]
pub mod task;
pub mod task_async;
pub mod task_context;
pub mod task_id;
mod task_macros;
pub mod task_sync;
pub mod task_trace;
#[cfg(test)]
mod task_trace_test;
mod test_debug_server;

// Re-exported so the `task_trace!` macro can refer to it via `$crate`
// without forcing every caller to depend on `async-backtrace` directly.
pub use async_backtrace;
pub mod this_file;
pub mod thread_trace;
pub mod toml;

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
