use std::future::Future;

/// Owned handle to a tokio runtime.
///
/// The wrapped `tokio::runtime::Runtime` is intentionally not exposed publicly;
/// external code can only run futures via `block_on` or enter the runtime context.
pub struct TaskRuntime {
    inner: tokio::runtime::Runtime,
}

/// Guard that enters a tokio runtime context. When dropped the context is exited.
pub struct TaskRuntimeEnterGuard<'a> {
    _guard: tokio::runtime::EnterGuard<'a>,
}

impl TaskRuntime {
    pub(crate) fn new(inner: tokio::runtime::Runtime) -> Self {
        Self { inner }
    }

    pub(crate) fn as_tokio_runtime(&self) -> &tokio::runtime::Runtime {
        &self.inner
    }

    pub fn block_on<F>(&self, future: F) -> F::Output
    where
        F: Future,
    {
        self.inner.block_on(future)
    }

    pub fn enter(&self) -> TaskRuntimeEnterGuard<'_> {
        TaskRuntimeEnterGuard {
            _guard: self.inner.enter(),
        }
    }
}

use super::spawn_local::{spawn_local_detached, LocalTaskSet};
use crate::imp::sync::async_::Notifier;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use tokio::task::LocalSet;
use tracing::{error, info};

pub fn build_current_thread_runtime() -> RS<TaskRuntime> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map(TaskRuntime::new)
        .map_err(|e| mudu_error!(ErrorCode::Tokio, "create current thread runtime error", e))
}

pub fn build_multi_thread_runtime() -> RS<TaskRuntime> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map(TaskRuntime::new)
        .map_err(|e| mudu_error!(ErrorCode::Tokio, "create multi thread runtime error", e))
}

pub struct CurrentThreadTaskRuntime {
    runtime: TaskRuntime,
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

    #[expect(
        clippy::unwrap_used,
        reason = "CurrentThreadTaskRuntime::block_on returns F::Output directly; errors are propagated through the task join handle"
    )]
    pub fn block_on<F>(&self, future: F) -> F::Output
    where
        F: Future + 'static,
        F::Output: 'static,
    {
        __block_on_tokio_current_thread_runtime(&self.runtime, self.local.inner(), future).unwrap()
    }
}

pub fn block_on_tokio_current_thread<F>(fut: F) -> RS<F::Output>
where
    F: Future + 'static,
    F::Output: 'static,
{
    let runtime = build_current_thread_runtime()?;
    let ls = LocalSet::new();
    __block_on_tokio_current_thread_runtime(&runtime, &ls, fut)
}

pub fn __block_on_tokio_current_thread_runtime<F>(
    runtime: &TaskRuntime,
    ls: &LocalSet,
    fut: F,
) -> RS<F::Output>
where
    F: Future + 'static,
    F::Output: 'static,
{
    let task = ls.run_until(async move {
        let join = spawn_local_detached("block-on", fut)?;
        join.await
            .map_err(|e| mudu_error!(ErrorCode::Tokio, "task runtime error", e))
    });
    let r = runtime.block_on(task);
    let opt = r.map_err(|e| mudu_error!(ErrorCode::Tokio, "tokio error", e))?;
    match opt {
        None => Err(mudu_error!(ErrorCode::Tokio, "return none")),
        Some(output) => Ok(output),
    }
}

#[expect(
    clippy::unwrap_used,
    reason = "block_on_async_current returns F::Output directly; errors are propagated through the task join handle"
)]
pub fn block_on_async_current<F>(fut: F) -> F::Output
where
    F: Future + 'static,
    F::Output: 'static,
{
    block_on_tokio_current_thread(fut).unwrap()
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

pub fn has_tokio_runtime() -> bool {
    tokio::runtime::Handle::try_current().is_ok()
}
