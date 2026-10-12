//! Worker execution layer: worker runtime, sessions, fs services, and config.

#![warn(missing_docs)]

/// Async procedure invocation surface of a worker.
pub mod async_func_runtime;
pub mod fs_fd_table;
pub mod fs_gc;
#[cfg(any(test, feature = "test"))]
pub mod fs_gc_test;
pub mod fs_service;
#[cfg(any(test, feature = "test"))]
pub mod fs_service_test;
pub mod loop_stats;
/// Request/response API a worker exposes to connection handlers.
pub mod request_response_worker;
/// Session-to-worker routing configuration.
pub mod routing;
/// Server configuration shared by the TCP backends.
pub mod server_cfg;
/// The per-worker execution runtime.
pub mod worker;
/// Counter snapshot of the io_uring worker event loop.
pub mod worker_loop_stats;
/// Persistent worker identities and topology lookup.
pub mod worker_registry;
/// Connection-scoped session registry of a worker.
pub mod worker_session_manager;
