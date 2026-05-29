use crate::env::default_env;
use crate::sync_async::notify_wait;
use crate::sync_async::Notifier;
use crate::sync_async::NotifyWait;
use crate::task_context::TaskContext;
use crate::task_id::{self, TaskID};
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use std::cell::Cell;
use std::future::Future;
use std::time::Duration;
use tokio::runtime::Runtime;
use tokio::task::{JoinHandle, LocalSet};
use tokio::time::sleep as tokio_sleep;
use tokio::{select, task, task_local};

use tracing::trace;
use tracing::{error, info};

task_local! {
    static TASK_ID: TaskID;
}

thread_local! {
    static CURRENT_POLL_TASK_ID: Cell<Option<TaskID>> = const { Cell::new(None) };
}

pub struct PollTaskIdGuard {
    prev: Option<TaskID>,
}

impl PollTaskIdGuard {
    pub fn enter(id: TaskID) -> Self {
        let prev = CURRENT_POLL_TASK_ID.with(|slot| {
            let prev = slot.get();
            slot.set(Some(id));
            prev
        });
        Self { prev }
    }
}

impl Drop for PollTaskIdGuard {
    fn drop(&mut self) {
        CURRENT_POLL_TASK_ID.with(|slot| {
            slot.set(self.prev);
        });
    }
}

pub async fn sleep(dur: Duration) -> RS<()> {
    default_env().task_async().sleep(dur).await
}

pub fn this_task_id() -> TaskID {
    try_this_task_id()
        .expect("cannot access task id: neither tokio task-local nor poll-task TLS is set")
}

pub fn try_this_task_id() -> Option<TaskID> {
    TASK_ID
        .try_with(|id| *id)
        .ok()
        .or_else(current_poll_task_id)
}

pub fn current_poll_task_id() -> Option<TaskID> {
    CURRENT_POLL_TASK_ID.with(|slot| slot.get())
}

pub fn spawn_local_task<F>(
    cancel_notifier: NotifyWait,
    name: &str,
    future: F,
) -> RS<JoinHandle<Option<F::Output>>>
where
    F: Future + 'static,
    F::Output: 'static,
{
    let id = {
        let id = task_id::new_task_id();
        let _ = TaskContext::new_context(id, name.to_string(), false);
        id
    };
    Ok(task::spawn_local(TASK_ID.scope(id, async move {
        let result = __select_local_till_done(cancel_notifier, future).await;
        TaskContext::remove_context(id);
        result
    })))
}

pub fn spawn_local_detached<F>(name: &str, future: F) -> RS<JoinHandle<Option<F::Output>>>
where
    F: Future + 'static,
    F::Output: 'static,
{
    let (_cancel_notifier, cancel_waiter) = notify_wait();
    spawn_local_task(cancel_waiter.into(), name, future)
}

pub fn spawn_task<F>(
    cancel_notifier: NotifyWait,
    name: &str,
    future: F,
) -> RS<JoinHandle<Option<F::Output>>>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    let id = task_id::new_task_id();
    let _ = TaskContext::new_context(id, name.to_string(), false);
    Ok(task::spawn(TASK_ID.scope(id, async move {
        let result = __select_till_done(cancel_notifier, future).await;
        TaskContext::remove_context(id);
        result
    })))
}

pub fn spawn_local_task_timeout<F>(
    cancel_notifier: NotifyWait,
    duration: Duration,
    _name: &str,
    future: F,
) -> RS<JoinHandle<Result<F::Output, TaskFailed>>>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    Ok(task::spawn_local(async move {
        __select_local_till_done_or_timeout(cancel_notifier, duration, future).await
    }))
}

pub fn spawn_tokio<F>(fut: F) -> tokio::task::JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    tokio::spawn(fut)
}

pub async fn spawn_blocking<F, T>(f: F) -> RS<T>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| m_error!(EC::ThreadErr, "spawn_blocking join error", e))
}

pub async fn timeout<F>(dur: Duration, fut: F) -> Option<F::Output>
where
    F: Future,
{
    tokio::time::timeout(dur, fut).await.ok()
}

pub fn has_tokio_runtime() -> bool {
    tokio::runtime::Handle::try_current().is_ok()
}

pub fn build_current_thread_runtime() -> RS<Runtime> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| m_error!(EC::TokioErr, "create current thread runtime error", e))
}

pub fn build_multi_thread_runtime() -> RS<Runtime> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|e| m_error!(EC::TokioErr, "create multi thread runtime error", e))
}

pub struct LocalTaskSet {
    inner: LocalSet,
}

impl LocalTaskSet {
    pub fn new() -> Self {
        Self {
            inner: LocalSet::new(),
        }
    }

    pub fn spawn<F>(
        &self,
        cancel_notifier: NotifyWait,
        name: &str,
        future: F,
    ) -> RS<JoinHandle<Option<F::Output>>>
    where
        F: Future + 'static,
        F::Output: 'static,
    {
        let id = task_id::new_task_id();
        let _ = TaskContext::new_context(id, name.to_string(), false);
        Ok(self.inner.spawn_local(TASK_ID.scope(id, async move {
            let result = __select_local_till_done(cancel_notifier, future).await;
            TaskContext::remove_context(id);
            result
        })))
    }

    pub fn spawn_detached<F>(&self, name: &str, future: F) -> RS<JoinHandle<Option<F::Output>>>
    where
        F: Future + 'static,
        F::Output: 'static,
    {
        let (_cancel_notifier, cancel_waiter) = notify_wait();
        self.spawn(cancel_waiter.into(), name, future)
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

pub struct CurrentThreadTaskRuntime {
    runtime: Runtime,
    local: LocalTaskSet,
}

impl CurrentThreadTaskRuntime {
    pub fn new() -> RS<Self> {
        Ok(Self {
            runtime: build_current_thread_runtime()?,
            local: LocalTaskSet::new(),
        })
    }

    pub fn local(&self) -> &LocalTaskSet {
        &self.local
    }

    pub fn block_on<F>(&self, future: F) -> F::Output
    where
        F: Future + 'static,
        F::Output: 'static,
    {
        self.local.block_on(&self.runtime, future)
    }
}

pub fn block_on_tokio_current_thread<F>(fut: F) -> RS<F::Output>
where
    F: Future + 'static,
    F::Output: 'static,
{
    let runtime = build_current_thread_runtime()?;
    let ls = LocalSet::new();
    let task = ls.run_until(async move {
        let join = spawn_local_detached("block-on", fut)?;
        join.await
            .map_err(|e| m_error!(EC::TokioErr, "task runtime error", e))
    });
    let r = runtime.block_on(async move {
        let r = task.await;
        r
    });
    let opt = r.map_err(|e| m_error!(EC::TokioErr, "tokio tokio error", e))?;
    match opt {
        None => Err(m_error!(EC::TokioErr, "return none")),
        Some(output) => Ok(output),
    }
}

pub fn wait_for_shutdown_signal(stop: Notifier) {
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(e) => {
            error!("create runtime for signal listener error: {}", e);
            return;
        }
    };

    runtime.block_on(async move {
        let stop_wait = stop.clone().into();

        #[cfg(unix)]
        {
            use tokio::signal::unix::{signal, SignalKind};

            let mut sigterm = match signal(SignalKind::terminate()) {
                Ok(s) => s,
                Err(e) => {
                    error!("register SIGTERM handler error: {}", e);
                    return;
                }
            };

            tokio::select! {
                _ = stop_wait.notified() => {
                    return;
                }
                r = tokio::signal::ctrl_c() => {
                    if let Err(e) = r {
                        error!("register Ctrl-C handler error: {}", e);
                        return;
                    }
                    info!("received Ctrl-C/SIGINT, starting graceful shutdown");
                }
                _ = sigterm.recv() => {
                    info!("received SIGTERM, starting graceful shutdown");
                }
            }

            stop.notify_all();
            return;
        }

        #[cfg(not(unix))]
        {
            tokio::select! {
                _ = stop_wait.notified() => {
                    return;
                }
                r = tokio::signal::ctrl_c() => {
                    if let Err(e) = r {
                        error!("register Ctrl-C handler error: {}", e);
                        return;
                    }
                    info!("received Ctrl-C, starting graceful shutdown");
                    stop.notify_all();
                }
            }
        }
    });
}

pub enum TaskFailed {
    Cancel,
    Timeout,
}

async fn __select_local_till_done<F>(notify: NotifyWait, future: F) -> Option<F::Output>
where
    F: Future + 'static,
    F::Output: 'static,
{
    async move {
        select! {
            _ = notify.notified() => {
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

async fn __select_local_till_done_or_timeout<F>(
    notify: NotifyWait,
    duration: Duration,
    future: F,
) -> Result<F::Output, TaskFailed>
where
    F: Future + 'static,
    F::Output: 'static,
{
    async move {
        select! {
            _ = notify.notified() => {
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

async fn __select_till_done<F>(notify: NotifyWait, future: F) -> Option<F::Output>
where
    F: Future + 'static,
    F::Output: Send + 'static,
{
    async move {
        select! {
            _ = notify.notified() => {
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
