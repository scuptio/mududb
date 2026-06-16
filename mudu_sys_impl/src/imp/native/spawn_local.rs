use crate::sync::async_::Waiter;
use crate::task::async_::TASK_ID;
use crate::task::context::TaskContext;
use crate::task::id;
use mudu::common::result::RS;
use std::future::Future;
use std::time::Duration;
use tokio::runtime::Runtime;
use tokio::select;
use tokio::task::{JoinHandle, LocalSet};
use tokio::time::sleep as tokio_sleep;
use tracing::trace;

pub fn spawn_local_task<F>(
    cancel_waiter: Waiter,
    name: &str,
    future: F,
) -> RS<JoinHandle<Option<F::Output>>>
where
    F: Future + 'static,
    F::Output: 'static,
{
    let task_id = {
        let task_id = id::new_task_id();
        let _ = TaskContext::new_tokio_context(task_id, name.to_string());
        task_id
    };
    Ok(tokio::task::spawn_local(TASK_ID.scope(
        task_id,
        async move {
            let result = __select_local_till_done(cancel_waiter, future).await;
            TaskContext::remove_context(task_id);
            result
        },
    )))
}

pub fn spawn_local_detached<F>(name: &str, future: F) -> RS<JoinHandle<Option<F::Output>>>
where
    F: Future + 'static,
    F::Output: 'static,
{
    let (_cancel_notifier, cancel_waiter) = crate::sync::async_::notify_wait();
    spawn_local_task(cancel_waiter, name, future)
}

pub struct LocalTaskSet {
    inner: LocalSet,
}

impl Default for LocalTaskSet {
    fn default() -> Self {
        Self::new()
    }
}

impl LocalTaskSet {
    pub fn inner(&self) -> &LocalSet {
        &self.inner
    }

    pub fn new() -> Self {
        Self {
            inner: LocalSet::new(),
        }
    }

    pub fn spawn<F>(
        &self,
        cancel_waiter: Waiter,
        name: &str,
        future: F,
    ) -> RS<JoinHandle<Option<F::Output>>>
    where
        F: Future + 'static,
        F::Output: 'static,
    {
        let task_id = id::new_task_id();
        let _ = TaskContext::new_tokio_context(task_id, name.to_string());
        Ok(self.inner.spawn_local(TASK_ID.scope(task_id, async move {
            let result = __select_local_till_done(cancel_waiter, future).await;
            TaskContext::remove_context(task_id);
            result
        })))
    }

    pub fn spawn_detached<F>(&self, name: &str, future: F) -> RS<JoinHandle<Option<F::Output>>>
    where
        F: Future + 'static,
        F::Output: 'static,
    {
        let (_cancel_notifier, cancel_waiter) = crate::sync::async_::notify_wait();
        self.spawn(cancel_waiter, name, future)
    }

    pub async fn run_until<F>(&self, future: F) -> F::Output
    where
        F: Future,
    {
        self.inner.run_until(future).await
    }

    pub fn block_on<F>(&self, runtime: &Runtime, future: F) -> F::Output
    where
        F: Future,
    {
        self.inner.block_on(runtime, future)
    }
}

async fn __select_local_till_done<F>(waiter: Waiter, future: F) -> Option<F::Output>
where
    F: Future + 'static,
    F::Output: 'static,
{
    async move {
        select! {
            _ = waiter.wait() => {
                trace!("local task stop");
                None
            }
            result = future => {
                trace!("local task end");
                Some(result)
            }
        }
    }
    .await
}

pub async fn __select_local_till_done_or_timeout<F>(
    waiter: Waiter,
    duration: Duration,
    future: F,
) -> Result<F::Output, TaskFailed>
where
    F: Future + 'static,
    F::Output: 'static,
{
    async move {
        select! {
            _ = waiter.wait() => {
                trace!("local task stop");
                Err(TaskFailed::Cancel)
            }
            result = future => {
                trace!("local task end");
                Ok(result)
            }
            _ = tokio_sleep(duration) => Err(TaskFailed::Timeout),
        }
    }
    .await
}

#[derive(Debug)]
pub enum TaskFailed {
    Cancel,
    Timeout,
}
