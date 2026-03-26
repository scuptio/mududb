use crate::notifier::{NotifyWait, Waiter};
use crate::sync::unique_inner::UniqueInner;
use crate::task::{spawn_local_task, spawn_task};
use futures::future::select_all;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use std::any::Any;
use std::pin::Pin;
use tokio::task::{JoinHandle, LocalSet};
pub trait Task: Any {}

pub trait AsyncTask: Task + Send + Sync {
    fn notifier(&self) -> NotifyWait;

    fn name(&self) -> String;

    fn async_run(self) -> impl Future<Output = RS<()>> + Send;
}

// A-synchronized task run in local thread
pub trait AsyncLocalTask: Task {
    fn waiter(&self) -> Waiter;

    fn name(&self) -> String;

    fn async_run_local(self) -> impl Future<Output = RS<()>>;
}

trait AsyncWrapper {
    fn async_run(&self) -> RS<(Option<LocalSet>, JoinHandle<Option<RS<()>>>)>;

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

    fn task_async_run(&self) -> RS<(Option<LocalSet>, JoinHandle<Option<RS<()>>>)> {
        let t = self.inner.inner_into();
        let join = spawn_task(t.notifier(), t.name().as_str(), async move {
            t.async_run().await
        });
        Ok((None, join?))
    }

    fn task_name(&self) -> Option<String> {
        self.inner.map_inner(|e| e.name().clone())
    }
}

struct AsyncLocalTaskWrapper<T: AsyncLocalTask + 'static> {
    inner: UniqueInner<(LocalSet, T)>,
}

impl<T: AsyncLocalTask + 'static> AsyncLocalTaskWrapper<T> {
    fn new(ls: LocalSet, inner: T) -> Self {
        Self {
            inner: UniqueInner::new((ls, inner)),
        }
    }

    fn task_async_run(&self) -> RS<(Option<LocalSet>, JoinHandle<Option<RS<()>>>)> {
        let (ls, t) = self.inner.inner_into();
        let join = ls.spawn_local(async move {
            let join = spawn_local_task(t.waiter().into(), t.name().as_str(), async move {
                t.async_run_local().await
            });
            let opt = join.unwrap().await.unwrap();
            opt
        });
        Ok((Some(ls), join))
    }

    fn task_name(&self) -> Option<String> {
        self.inner.map_inner(|e| e.1.name().clone())
    }
}

impl<T: AsyncLocalTask + 'static> AsyncWrapper for AsyncLocalTaskWrapper<T> {
    fn async_run(&self) -> RS<(Option<LocalSet>, JoinHandle<Option<RS<()>>>)> {
        self.task_async_run()
    }

    fn name(&self) -> Option<String> {
        self.task_name()
    }
}

impl<T: AsyncTask + 'static> AsyncWrapper for AsyncTaskWrapper<T> {
    fn async_run(&self) -> RS<(Option<LocalSet>, JoinHandle<Option<RS<()>>>)> {
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
    opt_local: Option<LocalSet>,
    join_handle: JoinHandle<Option<RS<()>>>,
}

impl TaskWrapper {
    pub fn new_async<T: AsyncTask + 'static>(t: T) -> Self {
        Self {
            inner: Box::new(AsyncTaskWrapper::new(t)),
        }
    }

    pub fn new_async_local<T: AsyncLocalTask + 'static>(ls: LocalSet, t: T) -> Self {
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
        let mut result = result;
        let mut local_sets = vec![];
        for r in result.iter_mut() {
            let mut opt = None;
            std::mem::swap(&mut r.opt_local, &mut opt);
            match opt {
                Some(r) => {
                    local_sets.push(r);
                }
                None => {}
            }
        }
        let mut pinned_futures: Vec<Pin<Box<dyn Future<Output = ()>>>> = Vec::new();

        for ls in local_sets {
            let future = async move {
                ls.run_until(std::future::pending::<()>()).await;
            };
            pinned_futures.push(Box::pin(future));
        }
        // wait local set
        while !pinned_futures.is_empty() {
            let (_, index, remaining) = select_all(pinned_futures).await;
            println!(
                "One LocalSet {} mpleted, {} remaining",
                index,
                remaining.len()
            );
            pinned_futures = remaining;
        }
        // wait all task join
        for r in result.into_iter() {
            let _opt = r
                .join_handle
                .await
                .map_err(|e| m_error!(EC::InternalErr, "join error", e))?;
        }

        Ok(())
    }

    pub fn name(&self) -> Option<String> {
        self.inner.name()
    }
}

unsafe impl Send for TaskWrapper {}
unsafe impl Sync for TaskWrapper {}
