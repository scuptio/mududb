use super::{Notifier, Waiter};
use crate::imp::sync::sync::unique_inner::UniqueInner;
use crate::task::async_::{spawn_task, LocalTaskSet, TaskJoinHandle};
use futures::future::try_join_all;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::error::MuduError;
use mudu::mudu_error;
use std::any::Any;
use std::future::Future;
/// Base trait for named background tasks.
pub trait Task: Any {}

/// A background task that can run on any Send executor thread.
pub trait AsyncTask: Task + Send + Sync {
    /// Returns a notifier used to signal cancellation to the task.
    fn cancel_notifier(&self) -> Notifier;

    /// Returns the task name used for tracing.
    fn name(&self) -> String;

    /// Runs the task body to completion.
    fn async_run(self) -> impl Future<Output = RS<()>> + Send;
}

/// A background task that must run on a [`LocalTaskSet`] (its future is not
/// `Send`).
// A-synchronized task run in local thread
pub trait AsyncLocalTask: Task {
    /// Returns the waiter the task observes for cancellation.
    fn waiter(&self) -> Waiter;

    /// Returns the task name used for tracing.
    fn name(&self) -> String;

    /// Runs the task body to completion on the local task set.
    fn async_run_local(self) -> impl Future<Output = RS<()>>;
}

type TaskRunOutput = (Option<LocalTaskSet>, TaskJoinHandle<Option<RS<()>>>);

trait AsyncWrapper {
    fn async_run(&self) -> RS<TaskRunOutput>;

    fn name(&self) -> Option<String>;
}

struct AsyncTaskWrapper<T: AsyncTask + 'static> {
    inner: UniqueInner<T>,
}
impl<T: AsyncTask + 'static> AsyncTaskWrapper<T> {
    fn new(inner: T) -> Self {
        Self {
            inner: UniqueInner::new(inner),
        }
    }

    fn task_async_run(&self) -> RS<TaskRunOutput> {
        let t = self.inner.inner_into()?;
        let join = spawn_task(
            t.cancel_notifier().as_waiter(),
            t.name().as_str(),
            async move { t.async_run().await },
        );
        Ok((None, join?))
    }

    fn task_name(&self) -> Option<String> {
        self.inner.map_inner(|e| e.name().clone())
    }
}

struct AsyncLocalTaskWrapper<T: AsyncLocalTask + 'static> {
    inner: UniqueInner<(LocalTaskSet, T)>,
}

impl<T: AsyncLocalTask + 'static> AsyncLocalTaskWrapper<T> {
    fn new(ls: LocalTaskSet, inner: T) -> Self {
        Self {
            inner: UniqueInner::new((ls, inner)),
        }
    }

    fn task_async_run(&self) -> RS<TaskRunOutput> {
        let (ls, t) = self.inner.inner_into()?;
        let join = ls.spawn(t.waiter(), t.name().as_str(), async move {
            t.async_run_local().await
        })?;
        Ok((Some(ls), join))
    }

    fn task_name(&self) -> Option<String> {
        self.inner.map_inner(|e| e.1.name().clone())
    }
}

impl<T: AsyncLocalTask + 'static> AsyncWrapper for AsyncLocalTaskWrapper<T> {
    fn async_run(&self) -> RS<TaskRunOutput> {
        self.task_async_run()
    }

    fn name(&self) -> Option<String> {
        self.task_name()
    }
}

impl<T: AsyncTask + 'static> AsyncWrapper for AsyncTaskWrapper<T> {
    fn async_run(&self) -> RS<TaskRunOutput> {
        self.task_async_run()
    }

    fn name(&self) -> Option<String> {
        self.task_name()
    }
}
/// Type-erased spawnable wrapper around an [`AsyncTask`] or
/// [`AsyncLocalTask`].
pub struct TaskWrapper {
    inner: Box<dyn AsyncWrapper>,
}

/// Handle to a spawned task: the join handle resolves to `None` if the task
/// was cancelled via its notifier, `Some(result)` otherwise.
pub struct AsyncResult {
    opt_local: Option<LocalTaskSet>,
    join_handle: TaskJoinHandle<Option<RS<()>>>,
}

impl TaskWrapper {
    /// Wraps a Send-capable task, ready to be spawned with
    /// [`TaskWrapper::async_run`].
    pub fn spawn_async<T: AsyncTask + 'static>(t: T) -> Self {
        Self {
            inner: Box::new(AsyncTaskWrapper::new(t)),
        }
    }

    /// Wraps a `!Send` task bound to the local task set `ls`, ready to be
    /// spawned with [`TaskWrapper::async_run`].
    pub fn spawn_async_local<T: AsyncLocalTask + 'static>(ls: LocalTaskSet, t: T) -> Self {
        Self {
            inner: Box::new(AsyncLocalTaskWrapper::new(ls, t)),
        }
    }

    /// Spawns the wrapped task and returns its result handle.
    pub fn async_run(&self) -> RS<AsyncResult> {
        let (opt_local, join_handle) = self.inner.async_run()?;
        Ok(AsyncResult {
            opt_local,
            join_handle,
        })
    }

    /// Waits for all spawned tasks to finish; fails if any task's join
    /// fails.
    pub async fn join_all(result: Vec<AsyncResult>) -> RS<()> {
        let futures = result.into_iter().map(|r| async move {
            let AsyncResult {
                opt_local,
                join_handle,
            } = r;
            match opt_local {
                Some(local_set) => {
                    let _opt = local_set
                        .run_until(async move {
                            join_handle
                                .await
                                .map_err(|e| mudu_error!(ErrorCode::Internal, "join error", e))
                        })
                        .await?;
                }
                None => {
                    let _opt = join_handle
                        .await
                        .map_err(|e| mudu_error!(ErrorCode::Internal, "join error", e))?;
                }
            }
            Ok::<(), MuduError>(())
        });
        try_join_all(futures).await?;
        Ok(())
    }

    /// Returns the name of the wrapped task, if it is still held.
    pub fn name(&self) -> Option<String> {
        self.inner.name()
    }
}

unsafe impl Send for TaskWrapper {}
unsafe impl Sync for TaskWrapper {}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    #![allow(clippy::expect_used)]
    #![allow(clippy::panic)]

    use super::super::notify_wait;
    use super::*;
    use crate::task::async_::LocalTaskSet;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    #[derive(Clone)]
    struct TestAsyncTask {
        notifier: Notifier,
        name: String,
        ran: Arc<AtomicBool>,
    }

    impl Task for TestAsyncTask {}

    impl AsyncTask for TestAsyncTask {
        fn cancel_notifier(&self) -> Notifier {
            self.notifier.clone()
        }

        fn name(&self) -> String {
            self.name.clone()
        }

        async fn async_run(self) -> RS<()> {
            tokio::task::yield_now().await;
            self.ran.store(true, Ordering::SeqCst);
            Ok(())
        }
    }

    struct TestAsyncLocalTask {
        waiter: Waiter,
        name: String,
        ran: Arc<AtomicBool>,
    }

    impl Task for TestAsyncLocalTask {}

    impl AsyncLocalTask for TestAsyncLocalTask {
        fn waiter(&self) -> Waiter {
            self.waiter.clone()
        }

        fn name(&self) -> String {
            self.name.clone()
        }

        async fn async_run_local(self) -> RS<()> {
            tokio::task::yield_now().await;
            self.ran.store(true, Ordering::SeqCst);
            Ok(())
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn async_task_wrapper_name_is_preserved() {
        let (notifier, _) = notify_wait();
        let task = TestAsyncTask {
            notifier,
            name: "my_async_task".to_string(),
            ran: Arc::new(AtomicBool::new(false)),
        };
        let wrapper = TaskWrapper::spawn_async(task);
        assert_eq!(wrapper.name(), Some("my_async_task".to_string()));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn async_task_runs_and_completes() {
        let ran = Arc::new(AtomicBool::new(false));
        let (notifier, _) = notify_wait();
        let task = TestAsyncTask {
            notifier,
            name: "async_run".to_string(),
            ran: ran.clone(),
        };
        let wrapper = TaskWrapper::spawn_async(task);
        let result = wrapper.async_run().unwrap();
        result.join_handle.await.unwrap().unwrap().unwrap();
        assert!(ran.load(Ordering::SeqCst));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn async_local_task_runs_and_completes() {
        let ran = Arc::new(AtomicBool::new(false));
        let (_notifier, waiter) = notify_wait();
        let task = TestAsyncLocalTask {
            waiter,
            name: "local_run".to_string(),
            ran: ran.clone(),
        };
        let ls = LocalTaskSet::new();
        let wrapper = TaskWrapper::spawn_async_local(ls, task);
        let result = wrapper.async_run().unwrap();
        TaskWrapper::join_all(vec![result]).await.unwrap();
        assert!(ran.load(Ordering::SeqCst));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn join_all_waits_for_multiple_tasks() {
        let ran1 = Arc::new(AtomicBool::new(false));
        let ran2 = Arc::new(AtomicBool::new(false));
        let (n1, _) = notify_wait();
        let (n2, _) = notify_wait();
        let task1 = TestAsyncTask {
            notifier: n1,
            name: "t1".to_string(),
            ran: ran1.clone(),
        };
        let task2 = TestAsyncTask {
            notifier: n2,
            name: "t2".to_string(),
            ran: ran2.clone(),
        };
        let wrapper1 = TaskWrapper::spawn_async(task1);
        let wrapper2 = TaskWrapper::spawn_async(task2);
        let r1 = wrapper1.async_run().unwrap();
        let r2 = wrapper2.async_run().unwrap();
        TaskWrapper::join_all(vec![r1, r2]).await.unwrap();
        assert!(ran1.load(Ordering::SeqCst));
        assert!(ran2.load(Ordering::SeqCst));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn async_task_cancelled_before_run() {
        let ran = Arc::new(AtomicBool::new(false));
        let (notifier, _) = notify_wait();
        let task = TestAsyncTask {
            notifier: notifier.clone(),
            name: "cancelled".to_string(),
            ran: ran.clone(),
        };
        notifier.notify_all();
        let wrapper = TaskWrapper::spawn_async(task);
        let result = wrapper.async_run().unwrap();
        let opt = result.join_handle.await.unwrap();
        assert!(opt.is_none());
        assert!(!ran.load(Ordering::SeqCst));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn async_local_task_cancelled_before_run() {
        let ran = Arc::new(AtomicBool::new(false));
        let (notifier, waiter) = notify_wait();
        let task = TestAsyncLocalTask {
            waiter,
            name: "local_cancelled".to_string(),
            ran: ran.clone(),
        };
        notifier.notify_all();
        let ls = LocalTaskSet::new();
        let wrapper = TaskWrapper::spawn_async_local(ls, task);
        let result = wrapper.async_run().unwrap();
        let ls = result.opt_local.unwrap();
        let opt = ls.run_until(result.join_handle).await.unwrap();
        assert!(opt.is_none());
        assert!(!ran.load(Ordering::SeqCst));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn async_run_consumes_wrapper() {
        let (notifier, _) = notify_wait();
        let task = TestAsyncTask {
            notifier,
            name: "consume".to_string(),
            ran: Arc::new(AtomicBool::new(false)),
        };
        let wrapper = TaskWrapper::spawn_async(task);
        assert!(wrapper.async_run().is_ok());
        assert!(wrapper.async_run().is_err());
    }
}
