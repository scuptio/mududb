use crate::imp::sync::async_::Waiter;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use std::future::Future;
use tokio::select;
use tokio::task;
use tracing::trace;

use super::async_::TASK_ID;
use super::{id, TaskContext, TaskJoinHandle};

/// Spawns `future` as a named background task on the current Tokio runtime.
///
/// The task is registered in the task context registry for tracing, and the
/// join handle resolves to `None` if `cancel_waiter` fires before the future
/// completes.
pub fn spawn_task<F>(
    cancel_waiter: Waiter,
    name: &str,
    future: F,
) -> RS<TaskJoinHandle<Option<F::Output>>>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    let task_id = id::new_task_id();
    let _ = TaskContext::new_tokio_context(task_id, name.to_string());
    Ok(TaskJoinHandle::new(task::spawn(TASK_ID.scope(
        task_id,
        async move {
            let result = __select_till_done(cancel_waiter, future).await;
            TaskContext::remove_context(task_id);
            result
        },
    ))))
}

/// Spawns `future` as a named background task whose cancellation notifier
/// is discarded (the task runs to completion).
pub fn spawn_task_detached<F>(name: &str, future: F) -> RS<TaskJoinHandle<Option<F::Output>>>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    let (_cancel_notifier, cancel_waiter) = crate::imp::sync::async_::notify_wait();
    spawn_task(cancel_waiter, name, future)
}

/// Runs the blocking closure `f` on the Tokio blocking thread pool and
/// awaits its result.
pub async fn spawn_blocking<F, T>(f: F) -> RS<T>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    task::spawn_blocking(f)
        .await
        .map_err(|e| mudu_error!(ErrorCode::Thread, "spawn_blocking join error", e))
}

async fn __select_till_done<F>(waiter: Waiter, future: F) -> Option<F::Output>
where
    F: Future + 'static,
    F::Output: Send + 'static,
{
    async move {
        select! {
            _ = waiter.wait() => {
                trace!("task stop");
                None
            }
            result = future => {
                trace!("task end");
                Some(result)
            }
        }
    }
    .await
}
