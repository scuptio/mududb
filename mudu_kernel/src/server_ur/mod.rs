//! TCP server backend with a Linux-first `io_uring` implementation.
//!
//! The public `client` module name is kept for compatibility. On Linux the
//! backend uses the native `io_uring` worker loop; on other platforms the same
//! public API falls back to a portable thread-per-worker implementation.
//! Modules that depend on `rliburing` are therefore compiled only on Linux.

pub mod fsm;
#[cfg(target_os = "linux")]
mod inflight_op;
mod pending_procedure_invocation;
#[cfg(all(test, target_os = "linux"))]
mod perf_test;
pub mod procedure_runtime;
mod procedure_task;
mod procedure_task_waker;
pub mod routing;
pub mod server;
#[cfg(target_os = "linux")]
mod server_iouring;
#[cfg(target_os = "linux")]
mod transferred_connection;
#[cfg(target_os = "linux")]
mod worker_mailbox;
pub mod worker;
#[cfg(target_os = "linux")]
mod worker_connection;
pub mod worker_local;
#[cfg(target_os = "linux")]
mod worker_local_log;
#[cfg(target_os = "linux")]
mod worker_ring_loop;
