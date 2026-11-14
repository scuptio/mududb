use crate::imp::sync::async_::Waiter;
use mudu::common::result::RS;
use std::future::Future;
use std::time::Duration;
use tokio::select;
use tokio::task::LocalSet;
use tokio::time::sleep as tokio_sleep;
use tracing::trace;

use super::async_::TASK_ID;
use super::{id, TaskContext, TaskJoinHandle, TaskRuntime};

/// Spawns a `!Send` future as a named task on the current thread's local
/// task set; the join handle resolves to `None` if `cancel_waiter` fires
/// first.
pub fn spawn_local_task<F>(
    cancel_waiter: Waiter,
    name: &str,
    future: F,
) -> RS<TaskJoinHandle<Option<F::Output>>>
where
    F: Future + 'static,
    F::Output: 'static,
{
    let task_id = {
        let task_id = id::new_task_id();
        let _ = TaskContext::new_tokio_context(task_id, name.to_string());
        task_id
    };
    Ok(TaskJoinHandle::new(tokio::task::spawn_local(
        TASK_ID.scope(task_id, async move {
            let result = __select_local_till_done(cancel_waiter, future).await;
            TaskContext::remove_context(task_id);
            result
        }),
    )))
}

/// Spawns a `!Send` future as a named local task whose cancellation
/// notifier is discarded (the task runs to completion).
pub fn spawn_local_detached<F>(name: &str, future: F) -> RS<TaskJoinHandle<Option<F::Output>>>
where
    F: Future + 'static,
    F::Output: 'static,
{
    let (_cancel_notifier, cancel_waiter) = crate::imp::sync::async_::notify_wait();
    spawn_local_task(cancel_waiter, name, future)
}

/// Spawns a thread-confined (`!Send`) fire-and-forget task on the current
/// worker thread's scheduler: the io_uring worker task registry when this
/// thread has a worker ring installed, otherwise the active tokio
/// `LocalSet` (panicking when there is none, same as
/// `tokio::task::spawn_local`). The task's output is discarded; callers
/// deliver results through their own channels. A `background` task idles for
/// the worker's lifetime and never blocks the io_uring shutdown drain.
pub fn spawn_thread_confined_task<F>(name: &str, future: F) -> RS<()>
where
    F: Future<Output = ()> + 'static,
{
    spawn_thread_confined_task_as(name, future, false)
}

/// Like [`spawn_thread_confined_task`], but the task never blocks the
/// worker's shutdown drain (for perpetual listeners that are dropped with
/// the worker anyway).
pub fn spawn_thread_confined_background_task<F>(name: &str, future: F) -> RS<()>
where
    F: Future<Output = ()> + 'static,
{
    spawn_thread_confined_task_as(name, future, true)
}

fn spawn_thread_confined_task_as<F>(name: &str, future: F, background: bool) -> RS<()>
where
    F: Future<Output = ()> + 'static,
{
    if crate::imp::io::worker_ring::has_current_worker_ring() {
        let task_id = id::new_task_id();
        let _ = TaskContext::new_iouring_context(task_id, name.to_string());
        let future = Box::pin(async move {
            future.await;
            Ok(())
        });
        return crate::imp::io::worker_ring::with_current_ring(|ring| {
            let registry = ring.worker_task_registry();
            if background {
                registry.spawn_background_with_trace_id(task_id, future);
            } else {
                registry.spawn_with_trace_id(None, task_id, future);
            }
            Ok(())
        });
    }
    let _ = background;
    spawn_local_detached(name, future)?;
    Ok(())
}

/// A set of `!Send` local tasks driven together on one thread (wraps
/// [`tokio::task::LocalSet`]).
pub struct LocalTaskSet {
    inner: LocalSet,
}

impl Default for LocalTaskSet {
    fn default() -> Self {
        Self::new()
    }
}

impl LocalTaskSet {
    /// Returns the underlying [`tokio::task::LocalSet`].
    pub fn inner(&self) -> &LocalSet {
        &self.inner
    }

    /// Creates an empty local task set.
    pub fn new() -> Self {
        Self {
            inner: LocalSet::new(),
        }
    }

    /// Spawns a named local task in this set, cancelled via
    /// `cancel_waiter`.
    pub fn spawn<F>(
        &self,
        cancel_waiter: Waiter,
        name: &str,
        future: F,
    ) -> RS<TaskJoinHandle<Option<F::Output>>>
    where
        F: Future + 'static,
        F::Output: 'static,
    {
        let task_id = id::new_task_id();
        let _ = TaskContext::new_tokio_context(task_id, name.to_string());
        Ok(TaskJoinHandle::new(self.inner.spawn_local(TASK_ID.scope(
            task_id,
            async move {
                let result = __select_local_till_done(cancel_waiter, future).await;
                TaskContext::remove_context(task_id);
                result
            },
        ))))
    }

    /// Spawns a named local task whose cancellation notifier is discarded
    /// (the task runs to completion).
    pub fn spawn_detached<F>(&self, name: &str, future: F) -> RS<TaskJoinHandle<Option<F::Output>>>
    where
        F: Future + 'static,
        F::Output: 'static,
    {
        let (_cancel_notifier, cancel_waiter) = crate::imp::sync::async_::notify_wait();
        self.spawn(cancel_waiter, name, future)
    }

    /// Runs the tasks in this set until `future` completes and returns its
    /// output.
    pub async fn run_until<F>(&self, future: F) -> F::Output
    where
        F: Future,
    {
        self.inner.run_until(future).await
    }

    /// Runs the tasks in this set on `runtime` until `future` completes,
    /// blocking the current thread.
    pub fn block_on<F>(&self, runtime: &TaskRuntime, future: F) -> F::Output
    where
        F: Future,
    {
        self.inner.block_on(runtime.as_tokio_runtime(), future)
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

/// Awaits `future`, returning [`TaskFailed::Cancel`] if `waiter` fires
/// first or [`TaskFailed::Timeout`] if `duration` elapses first (internal
/// helper).
#[doc(hidden)]
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

/// Why a raced task finished without producing its output.
#[derive(Debug)]
pub enum TaskFailed {
    /// The task's cancellation waiter fired before it completed.
    Cancel,
    /// The deadline elapsed before the task completed.
    Timeout,
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::{__select_local_till_done_or_timeout, LocalTaskSet, TaskFailed};
    use std::time::Duration;

    #[tokio::test(flavor = "current_thread")]
    async fn local_task_set_spawn_detached_and_run_until() {
        let set = LocalTaskSet::default();
        let handle = set.spawn_detached("answer", async { 40 + 2 }).unwrap();
        let out = set.run_until(async move { handle.await.unwrap() }).await;
        assert_eq!(out, Some(42));
    }

    #[test]
    fn local_task_set_block_on_and_inner() {
        let runtime = super::super::build_current_thread_runtime().unwrap();
        let set = LocalTaskSet::new();
        let _inner = set.inner();
        let out = set.block_on(&runtime, async { 7 });
        assert_eq!(out, 7);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn select_local_till_done_or_timeout_covers_all_outcomes() {
        use crate::imp::sync::async_::notify_wait;

        let (_notifier, waiter) = notify_wait();
        let done =
            __select_local_till_done_or_timeout(waiter, Duration::from_secs(60), async { 1 }).await;
        assert!(matches!(done, Ok(1)));

        let (_notifier, waiter) = notify_wait();
        let timed_out =
            __select_local_till_done_or_timeout(waiter, Duration::from_millis(1), async {
                loop {
                    tokio::task::yield_now().await;
                }
            })
            .await;
        assert!(matches!(timed_out, Err(TaskFailed::Timeout)));

        let (notifier, waiter) = notify_wait();
        notifier.notify_all();
        let cancelled = __select_local_till_done_or_timeout(
            waiter,
            Duration::from_secs(60),
            std::future::pending::<()>(),
        )
        .await;
        assert!(matches!(cancelled, Err(TaskFailed::Cancel)));
    }
}
