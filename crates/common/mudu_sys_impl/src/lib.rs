#![warn(missing_docs)]
#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
#![deny(clippy::dbg_macro)]
#![warn(clippy::panic)]
#![warn(clippy::todo)]
#![warn(clippy::unimplemented)]

//! Native implementation of the `mudu` system interface.
//!
//! This crate provides concrete OS/IO abstractions (filesystem, networking,
//! process, threading, synchronization, time, etc.) used by the rest of the
//! `mudu` project. It is intended to be used through the `mudu_sys` and
//! `mudu_sys_contract` crates rather than directly.

/// Common types and re-exports used across `mudu_sys_impl`.
pub mod common;
/// Contracts (traits) that system implementations must satisfy.
pub mod contract;
/// Performance monitoring helpers re-exported from `mudu_sys_contract`.
pub use mudu_sys_contract::perf;
/// Default system environment initialization.
pub mod env;
/// Environment variable and process environment helpers.
pub mod env_var;
/// Public filesystem abstractions and async file operations.
pub mod fs;
/// Concrete native implementations of the system contracts.
pub mod imp;
/// IO helpers and public re-exports.
pub mod io;
/// Public networking types and async/sync network operations.
pub mod net;
/// Process-related system operations.
pub mod process;
/// Async IO provider traits and dispatch.
pub mod provider;
/// Random value and UUID generation.
pub mod random;
/// Public synchronization primitives and blocking IO helpers.
pub mod sync;
/// System-wide IO context and provider selection.
pub mod sys_io_context;
/// Public task runtime, spawning, and blocking task helpers.
pub mod task;
mod task_macros;
/// Public time, instant, and datetime helpers.
pub mod time;

/// Default native system environment.
pub use crate::env::default_env;
/// System handle providing access to all native subsystems.
pub use crate::imp::env::Sys;
/// Native environment variable subsystem.
pub use crate::imp::env_var::SysEnvVar;
/// Native OS information subsystem.
pub use crate::imp::os::SysOs;
/// Native process subsystem.
pub use crate::imp::process::SysProcess;
/// Native random generation subsystem.
pub use crate::imp::random::SysRandom;
/// Native synchronization subsystem.
pub use crate::imp::sync::SysSync;
/// Native task subsystem.
pub use crate::imp::task::SysTasks;
/// Native thread subsystem.
pub use crate::imp::thread::SysThread;
/// Native time subsystem.
pub use crate::imp::time::SysTime;
/// Default system IO context and its handle.
pub use crate::sys_io_context::{default_sys_io_context, SysIoContext};

#[cfg(not(target_arch = "wasm32"))]
/// Async notification primitives.
pub use crate::sync::async_::{notify_wait, Notifier, NotifyWait, Waiter};
/// Blocking eventfd and file descriptor helpers.
pub use crate::sync::blocking::{close_fd, eventfd, notify_eventfd, read_eventfd};
/// Unbounded synchronous channel primitives.
pub use crate::sync::sync_::unbounded_channel::{unbounded_channel, ChannelSender, SyncReceiver};

#[cfg(not(target_arch = "wasm32"))]
/// Async task runtime helpers and types.
pub use crate::task::async_::{
    block_on_async_current, block_on_tokio_current_thread, build_current_thread_runtime,
    build_multi_thread_runtime, current_poll_task_id, has_tokio_runtime, sleep, spawn_blocking,
    spawn_local_detached, spawn_local_task, spawn_task, spawn_task_detached, this_task_id, timeout,
    try_this_task_id, wait_for_shutdown_signal, CurrentThreadTaskRuntime, LocalTaskSet,
    PollTaskIdGuard, TaskFailed, TaskJoinError, TaskJoinHandle, TaskRuntime, TaskRuntimeEnterGuard,
};
/// Per-task context used for tracing and debugging.
pub use crate::task::context::TaskContext;
/// Task ID generation and type alias.
pub use crate::task::id::{new_task_id, TaskID};
/// Blocking thread and sleep helpers.
pub use crate::task::sync::{sleep_blocking, spawn_thread, spawn_thread_named, SJoinHandle};
/// Task tracing abstractions.
pub use crate::task::trace::{NoopTaskTrace, TaskTrace};

#[cfg(not(target_arch = "wasm32"))]
/// Re-export of `tokio` for downstream async runtimes.
pub use tokio;

/// Worker task registry and async function task waker.
pub mod server;

/// Returns `true` if the Linux io_uring subsystem is available on this host.
pub fn io_uring_available() -> bool {
    use std::sync::OnceLock;
    static AVAILABLE: OnceLock<bool> = OnceLock::new();

    *AVAILABLE.get_or_init(|| {
        #[cfg(target_os = "linux")]
        {
            use std::sync::mpsc;
            use std::time::Duration;

            // Some containerized runners (e.g. act with a restrictive seccomp
            // profile) allow `io_uring_setup` but block `io_uring_enter`, which
            // makes a simple ring-creation check pass while any real work hangs.
            // Run a functional NOP probe on a separate thread and cap the wait so
            // that a blocked syscall is reported as unavailable instead of hanging
            // the caller.
            let (tx, rx) = mpsc::channel();
            std::thread::spawn(move || {
                let ok = crate::imp::native::linux::io_uring::iouring::IoUring::probe();
                let _ = tx.send(ok);
            });
            rx.recv_timeout(Duration::from_millis(1000))
                .unwrap_or(false)
        }
        #[cfg(not(target_os = "linux"))]
        {
            false
        }
    })
}
