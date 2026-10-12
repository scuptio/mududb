//! Async task/runtime facade used by higher-level crates.
pub use mudu_sys::task::async_::{
    CurrentThreadTaskRuntime, LocalTaskSet, PollTaskIdGuard, TaskFailed,
    block_on_tokio_current_thread, build_current_thread_runtime, build_multi_thread_runtime,
    current_poll_task_id, has_tokio_runtime, sleep, spawn_blocking, spawn_local_detached,
    spawn_local_task, spawn_task, spawn_task_detached, this_task_id, timeout, try_this_task_id,
    wait_for_shutdown_signal,
};

use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use std::future::Future;

/// Run an async future on a dedicated thread and block until it completes.
pub fn run_async<F, T>(future: F) -> RS<F::Output>
where
    F: Future<Output = T> + Send + 'static,
    T: Send + 'static,
{
    let thread = mudu_sys::task::sync::spawn_thread(move || -> RS<F::Output> {
        let runtime = build_multi_thread_runtime()?;
        Ok(runtime.block_on(future))
    })?;
    let r = thread
        .join()
        .map_err(|_e| mudu_error!(ErrorCode::Internal, "join thread error"))??;
    Ok(r)
}
