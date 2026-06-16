use crate::sync::async_::Waiter;
use crate::task::async_::TASK_ID;
use crate::task::context::TaskContext;
use crate::task::id;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use std::future::Future;
use tokio::select;
use tokio::task::{self, JoinHandle};
use tracing::trace;

pub fn spawn_task<F>(
    cancel_waiter: Waiter,
    name: &str,
    future: F,
) -> RS<JoinHandle<Option<F::Output>>>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    let task_id = id::new_task_id();
    let _ = TaskContext::new_tokio_context(task_id, name.to_string());
    Ok(task::spawn(TASK_ID.scope(task_id, async move {
        let result = __select_till_done(cancel_waiter, future).await;
        TaskContext::remove_context(task_id);
        result
    })))
}

#[allow(dead_code)]
pub fn spawn_task_detached<F>(name: &str, future: F) -> RS<JoinHandle<Option<F::Output>>>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    let (_cancel_notifier, cancel_waiter) = crate::sync::async_::notify_wait();
    spawn_task(cancel_waiter, name, future)
}

pub async fn spawn_blocking<F, T>(f: F) -> RS<T>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    task::spawn_blocking(f)
        .await
        .map_err(|e| m_error!(EC::ThreadErr, "spawn_blocking join error", e))
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
