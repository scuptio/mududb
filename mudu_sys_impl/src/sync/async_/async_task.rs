use crate::sync::async_::{Notifier, Waiter};
use crate::sync::unique_inner::UniqueInner;
use crate::task::async_::{LocalTaskSet, spawn_task};
use futures::future::try_join_all;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use std::any::Any;
use std::future::Future;
use tokio::task::JoinHandle;
pub trait Task: Any {}

pub trait AsyncTask: Task + Send + Sync {
    fn cancel_notifier(&self) -> Notifier;

    fn name(&self) -> String;

    fn async_run(self) -> impl Future<Output = RS<()>> + Send;
}

// A-synchronized task run in local thread
pub trait AsyncLocalTask: Task {
    fn waiter(&self) -> Waiter;

    fn name(&self) -> String;

    fn async_run_local(self) -> impl Future<Output = RS<()>>;
}

type TaskRunOutput = (Option<LocalTaskSet>, JoinHandle<Option<RS<()>>>);

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
        let t = self.inner.inner_into();
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
        let (ls, t) = self.inner.inner_into();
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
pub struct TaskWrapper {
    inner: Box<dyn AsyncWrapper>,
}

pub struct AsyncResult {
    opt_local: Option<LocalTaskSet>,
    join_handle: JoinHandle<Option<RS<()>>>,
}

impl TaskWrapper {
    pub fn spawn_async<T: AsyncTask + 'static>(t: T) -> Self {
        Self {
            inner: Box::new(AsyncTaskWrapper::new(t)),
        }
    }

    pub fn spawn_async_local<T: AsyncLocalTask + 'static>(ls: LocalTaskSet, t: T) -> Self {
        Self {
            inner: Box::new(AsyncLocalTaskWrapper::new(ls, t)),
        }
    }

    pub fn async_run(&self) -> RS<AsyncResult> {
        let (opt_local, join_handle) = self.inner.async_run()?;
        Ok(AsyncResult {
            opt_local,
            join_handle,
        })
    }

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
                                .map_err(|e| m_error!(EC::InternalErr, "join error", e))
                        })
                        .await?;
                }
                None => {
                    let _opt = join_handle
                        .await
                        .map_err(|e| m_error!(EC::InternalErr, "join error", e))?;
                }
            }
            Ok(())
        });
        try_join_all(futures).await?;
        Ok(())
    }

    pub fn name(&self) -> Option<String> {
        self.inner.name()
    }
}

unsafe impl Send for TaskWrapper {}
unsafe impl Sync for TaskWrapper {}
