//! TCP server backend with a Linux-first `io_uring` implementation.
//!
//! The public `client` module name is kept for compatibility. On Linux the
//! backend uses the native `io_uring` worker loop; on other platforms the same
//! public API falls back to a portable thread-per-worker implementation.
//! Modules that depend on `rliburing` are therefore compiled only on Linux.

#![warn(missing_docs)]
#![allow(clippy::module_inception)]
// Backwards-compatible re-exports. These items historically lived under
// `server` but were moved to lower modules to break module dependency cycles;
// external consumers (mudu_runtime, testing) keep importing them from the old
// `server::*` paths.
pub use ::catalog::contract::message_bus_api;
pub use ::catalog::contract::worker_snapshot;
pub use ::conn::worker_local;
pub use ::xengine::partition_router;

/// Asynchronous procedure invocation runtime, re-exported from the `worker`
/// crate.
pub mod async_func_runtime {
    pub use ::worker::async_func_runtime::*;
}
/// Session-scoped file descriptor tables for the fs object syscalls,
/// re-exported from the `worker` crate.
pub mod fs_fd_table {
    pub use ::worker::fs_fd_table::*;
}
/// Fs object generation garbage collection, re-exported from the `worker`
/// crate.
pub mod fs_gc {
    pub use ::worker::fs_gc::*;
}
/// Host-side fs object I/O service, re-exported from the `worker` crate.
pub mod fs_service {
    pub use ::worker::fs_service::*;
}
/// Event-loop phase accounting for the worker loop, re-exported from the
/// `worker` crate.
pub mod loop_stats {
    pub use ::worker::loop_stats::*;
}
/// Request/response worker driver traits, re-exported from the `worker`
/// crate.
pub mod request_response_worker {
    pub use ::worker::request_response_worker::*;
}
/// Session routing modes, re-exported from the `worker` crate.
pub mod routing {
    pub use ::worker::routing::*;
}
/// Server configuration types, re-exported from the `worker` crate.
pub mod server_cfg {
    pub use ::worker::server_cfg::*;
}
/// Worker runtime, re-exported from the `worker` crate.
pub mod worker {
    pub use ::worker::worker::*;
}
/// Worker loop statistics, re-exported from the `worker` crate.
pub mod worker_loop_stats {
    pub use ::worker::worker_loop_stats::*;
}
/// Persistent worker identity registry, re-exported from the `worker` crate.
pub mod worker_registry {
    pub use ::worker::worker_registry::*;
}
/// Per-worker session management, re-exported from the `worker` crate.
pub mod worker_session_manager {
    pub use ::worker::worker_session_manager::*;
}
/// Tests for the fs garbage collector, re-exported from the `worker` crate.
#[cfg(all(test, not(miri)))]
pub mod fs_gc_test {
    #[allow(unused_imports)]
    pub use ::worker::fs_gc_test::*;
}
/// Tests for the fs object I/O service, re-exported from the `worker` crate.
#[cfg(all(test, not(miri)))]
pub mod fs_service_test {
    #[allow(unused_imports)]
    pub use ::worker::fs_service_test::*;
}

pub mod app_schema_lookup;
mod async_func_task;
/// Backup management, re-exported from the `backup` crate.
pub mod backup {
    pub use ::backup::backup::*;
}
#[cfg(all(test, target_os = "linux"))]
#[path = "linux/callback_registry.rs"]
mod callback_registry;
pub mod connection_state;
#[cfg(target_os = "linux")]
#[path = "linux/connection_worker_task.rs"]
mod connection_worker_task;
mod frame_dispatch;
#[cfg(all(test, not(miri)))]
pub mod fs_e2e_test;
mod handlers;
#[cfg(target_os = "linux")]
#[path = "linux/inflight_op.rs"]
mod inflight_op;
/// Logical backup, re-exported from the `backup` crate.
pub mod logical_backup {
    pub use ::backup::logical_backup::*;
}
#[cfg(target_os = "linux")]
#[path = "linux/loop_mailbox.rs"]
mod loop_mailbox;
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
#[cfg(target_os = "linux")]
#[path = "linux/loop_user_io.rs"]
mod loop_user_io;
#[cfg(target_os = "linux")]
#[path = "linux/message_bus_runtime.rs"]
mod message_bus_runtime;
mod message_bus_state;
mod message_dispatcher;
#[cfg(all(test, target_os = "linux"))]
#[path = "linux/perf_test.rs"]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod perf_test;
#[cfg(all(test, not(miri)))]
pub mod plan_cache_e2e_test;
mod procedure_runtimes;
#[cfg(target_os = "linux")]
#[path = "linux/protocol_codec.rs"]
mod protocol_codec;
/// Replication drivers and controls, re-exported from the `repl` crate.
pub mod repl {
    pub use ::repl::*;
}
mod request_ctx;
/// Backup restore, re-exported from the `backup` crate.
pub mod restore {
    pub use ::backup::restore::*;
}
pub mod server;
#[cfg(target_os = "linux")]
#[path = "linux/server_iouring.rs"]
mod server_iouring;
pub mod server_launch;
pub mod server_runtime_deps;
pub use ::catalog::stage_stats;
#[cfg(test)]
mod stmt_path_bench;
mod task;
#[cfg(target_os = "linux")]
#[path = "linux/worker_mailbox.rs"]
mod worker_mailbox;
#[cfg(target_os = "linux")]
#[path = "linux/worker_ring_loop/mod.rs"]
mod worker_ring_loop;
/// Storage engine contract, re-exported from the `xcontract` crate.
pub mod x_contract {
    pub use ::xcontract::*;
}
