//! Async I/O contracts for file system, network, listeners, streams,
//! task scheduling and provider composition.

/// Trait for asynchronous random-access file handles.
pub mod async_file;
/// Trait for asynchronous file system operations.
pub mod async_fs;
/// Trait that groups async network and file system providers.
pub mod async_io_provider;
/// Trait for asynchronous network listeners.
pub mod async_listener;
/// Runtime modes supported by async I/O providers.
pub mod async_mode;
/// Trait for asynchronous network operations.
pub mod async_net;
/// Trait for asynchronous bidirectional byte streams.
pub mod async_stream;
/// Options controlling how a file is opened.
pub mod file_options;
/// Generic base implementation of an async I/O provider.
pub mod io_provider_base;
/// Trait for async task scheduling primitives.
pub mod task_async;
