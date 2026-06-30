//! TCP server backend with a Linux-first `io_uring` implementation.
//!
//! The public `client` module name is kept for compatibility. On Linux the
//! backend uses the native `io_uring` worker loop; on other platforms the same
//! public API falls back to a portable thread-per-worker implementation.
//! Modules that depend on `rliburing` are therefore compiled only on Linux.

#![allow(missing_docs)]
#![allow(clippy::module_inception)]
pub mod async_func_runtime;
mod async_func_task;
#[cfg(all(test, target_os = "linux"))]
#[path = "linux/callback_registry.rs"]
mod callback_registry;
pub mod connection_state;
#[cfg(target_os = "linux")]
#[path = "linux/connection_worker_task.rs"]
mod connection_worker_task;
mod frame_dispatch;
mod handlers;
#[cfg(target_os = "linux")]
#[path = "linux/inflight_op.rs"]
mod inflight_op;
#[cfg(target_os = "linux")]
#[path = "linux/loop_mailbox.rs"]
mod loop_mailbox;
#[cfg(target_os = "linux")]
#[path = "linux/loop_user_io.rs"]
mod loop_user_io;
pub mod message_bus_api;
#[cfg(target_os = "linux")]
#[path = "linux/message_bus_runtime.rs"]
mod message_bus_runtime;
mod message_bus_state;
mod message_dispatcher;
pub mod partition_router;
mod partition_rpc;
#[cfg(all(test, target_os = "linux"))]
#[path = "linux/perf_test.rs"]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod perf_test;
mod procedure_runtimes;
#[cfg(target_os = "linux")]
#[path = "linux/protocol_codec.rs"]
mod protocol_codec;
mod request_ctx;
mod request_response_worker;
pub mod routing;
pub mod server;
pub mod server_cfg;
#[cfg(target_os = "linux")]
#[path = "linux/server_iouring.rs"]
mod server_iouring;
pub mod server_launch;
pub mod server_runtime_deps;
mod session_bound_worker_runtime;
mod task;
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
pub(crate) mod test_meta_mgr;
pub mod worker;
pub mod worker_local;
mod worker_loop_stats;
#[cfg(target_os = "linux")]
#[path = "linux/worker_mailbox.rs"]
mod worker_mailbox;
pub mod worker_registry;
#[cfg(target_os = "linux")]
#[path = "linux/worker_ring_loop.rs"]
mod worker_ring_loop;
mod worker_session_manager;
pub mod worker_snapshot;
mod worker_storage;
mod worker_tx_manager;
pub mod x_contract;
mod x_lock_mgr;
