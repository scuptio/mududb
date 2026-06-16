use super::spawn_local::{LocalTaskSet, spawn_local_detached};
use crate::sync::async_::Notifier;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use std::future::Future;
use tokio::runtime::Runtime;
use tokio::task::LocalSet;
use tracing::{error, info};

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
    runtime: &Runtime,
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
            .map_err(|e| m_error!(EC::TokioErr, "task runtime error", e))
    });
    let r = runtime.block_on(async move {
        let r = task.await;
        r
    });
    let opt = r.map_err(|e| m_error!(EC::TokioErr, "tokio error", e))?;
    match opt {
        None => Err(m_error!(EC::TokioErr, "return none")),
        Some(output) => Ok(output),
    }
}
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
            use tokio::signal::unix::{SignalKind, signal};

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
