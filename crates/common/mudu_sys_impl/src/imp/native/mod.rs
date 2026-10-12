/// System environment aggregate (`Sys`).
pub mod env;
/// Environment variable subsystem.
pub mod env_var;
/// Filesystem subsystem (sync facade, async providers).
pub mod fs;
/// IO helpers (raw fds, socket addresses, op state, worker ring access).
pub mod io;
/// Linux-specific subsystems (io_uring).
#[cfg(target_os = "linux")]
pub mod linux;
/// Networking subsystem (Tokio and io_uring providers).
pub mod net;
/// OS-specific operations.
pub mod os;
/// Process subsystem.
pub mod process;
/// Random generation subsystem.
pub mod random;
/// Async IO provider construction and dispatch.
pub mod runtime;
/// Synchronization subsystem (blocking and async primitives).
pub mod sync;
/// Async task subsystem over Tokio.
pub mod task;
/// Thread subsystem.
pub mod thread;
/// Time subsystem.
pub mod time;
