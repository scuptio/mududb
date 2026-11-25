//! Shared protocol owner executor and bounded Tokio blocking IO pool.

use std::future::Future;
use std::sync::OnceLock;

use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_sys::task::async_::build_serial_io_runtime;
use mudu_sys::task::async_::TaskRuntime;

/// Spawns a task on the shared owner executor.
pub fn spawn(future: impl Future<Output = ()> + Send + 'static) -> RS<()> {
    // The runtime lives for the process lifetime. All groups share its one
    // protocol thread and its bounded pool, rather than allocating a pool
    // or persistence thread per group.
    static RUNTIME: OnceLock<Result<TaskRuntime, String>> = OnceLock::new();
    let runtime = RUNTIME
        .get_or_init(|| build_serial_io_runtime(4).map_err(|e| e.to_string()))
        .as_ref()
        .map_err(|e| mudu_error!(ErrorCode::Tokio, e.clone()))?;
    let _enter = runtime.enter();
    mudu_sys::task::async_::spawn_task_detached("raft-driver", future)?;
    Ok(())
}

/// Errors when called from a tokio runtime worker thread.
pub fn ensure_sync_wait() -> RS<()> {
    if mudu_sys::task::async_::has_tokio_runtime() {
        return Err(mudu_error!(
            ErrorCode::InvalidState,
            "use the asynchronous Raft API from an async executor"
        ));
    }
    Ok(())
}
