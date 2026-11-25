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

    /// Runs `future` to completion on this runtime, blocking the current
    /// thread.
    pub fn block_on<F>(&self, future: F) -> F::Output
    where
        F: Future,
    {
        self.inner.block_on(future)
    }

    /// Enters the runtime context (so Tokio APIs work) until the returned
    /// guard drops.
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

/// Builds a current-thread Tokio runtime with all features enabled.
pub fn build_current_thread_runtime() -> RS<TaskRuntime> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map(TaskRuntime::new)
        .map_err(|e| mudu_error!(ErrorCode::Tokio, "create current thread runtime error", e))
}

/// Builds a multi-thread Tokio runtime with all features enabled.
pub fn build_multi_thread_runtime() -> RS<TaskRuntime> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map(TaskRuntime::new)
        .map_err(|e| mudu_error!(ErrorCode::Tokio, "create multi thread runtime error", e))
}

/// Builds one protocol owner executor with a bounded, shared blocking IO pool.
/// Multiple serialized protocol tasks can share this runtime.
pub fn build_serial_io_runtime(max_blocking_threads: usize) -> RS<TaskRuntime> {
    if max_blocking_threads == 0 {
        return Err(mudu_error!(
            ErrorCode::InvalidArgument,
            "blocking IO thread limit must be positive"
        ));
    }
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .max_blocking_threads(max_blocking_threads)
        .thread_name("mudu-protocol")
        .enable_all()
        .build()
        .map(TaskRuntime::new)
        .map_err(|e| mudu_error!(ErrorCode::Tokio, "create serial IO runtime error", e))
}

/// A current-thread Tokio runtime paired with a [`LocalTaskSet`] for
/// running `!Send` futures.
pub struct CurrentThreadTaskRuntime {
    runtime: TaskRuntime,
    local: LocalTaskSet,
}

impl CurrentThreadTaskRuntime {
    /// Creates a current-thread runtime and its local task set.
    pub fn new() -> RS<Self> {
        Ok(Self {
            runtime: build_current_thread_runtime()?,
            local: LocalTaskSet::new(),
        })
    }

    /// Creates a local runtime with a bounded blocking IO pool.
    pub fn with_max_blocking_threads(max_blocking_threads: usize) -> RS<Self> {
        if max_blocking_threads == 0 {
            return Err(mudu_error!(
                ErrorCode::InvalidArgument,
                "blocking IO thread limit must be positive"
            ));
        }
        let inner = tokio::runtime::Builder::new_current_thread()
            .max_blocking_threads(max_blocking_threads)
            .enable_all()
            .build()
            .map_err(|e| mudu_error!(ErrorCode::Tokio, "create local IO runtime error", e))?;
        Ok(Self {
            runtime: TaskRuntime::new(inner),
            local: LocalTaskSet::new(),
        })
    }

    /// Returns the local task set for spawning `!Send` futures.
    pub fn local(&self) -> &LocalTaskSet {
        &self.local
    }

    /// Runs `future` to completion on the local task set, blocking the
    /// current thread. Spawn failures panic; task join errors are
    /// propagated through the future's output type by the caller.
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

/// Runs `future` on a freshly built current-thread runtime with a fresh
/// local set, blocking the current thread.
pub fn block_on_tokio_current_thread<F>(fut: F) -> RS<F::Output>
where
    F: Future + 'static,
    F::Output: 'static,
{
    let runtime = build_current_thread_runtime()?;
    let ls = LocalSet::new();
    __block_on_tokio_current_thread_runtime(&runtime, &ls, fut)
}

/// Runs `fut` on `runtime` inside local set `ls` (internal helper shared by
/// the `block_on` entry points).
#[doc(hidden)]
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

/// Runs `fut` on a freshly built current-thread runtime, blocking the
/// current thread; panics if the runtime cannot be built or the task fails.
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

/// Blocks until Ctrl-C/SIGTERM (Unix) or Ctrl-C (other platforms) is
/// received or `stop` is notified, then notifies `stop` so dependents shut
/// down. Runs its own temporary current-thread runtime.
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

/// Returns `true` if called from within a Tokio runtime context.
pub fn has_tokio_runtime() -> bool {
    tokio::runtime::Handle::try_current().is_ok()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn runtime_block_on_enter_and_tokio_handle() {
        let runtime = build_current_thread_runtime().unwrap();
        assert_eq!(runtime.block_on(async { 2 }), 2);
        let _tokio = runtime.as_tokio_runtime();
        {
            let _guard = runtime.enter();
        }
        let multi = build_multi_thread_runtime().unwrap();
        assert_eq!(multi.block_on(async { 3 }), 3);
    }

    #[test]
    fn current_thread_task_runtime_local_and_block_on() {
        let crt = CurrentThreadTaskRuntime::new().unwrap();
        let _local = crt.local();
        assert_eq!(crt.block_on(async { 5 }), 5);
    }

    #[test]
    fn bounded_local_runtime_supports_thread_confined_tasks() {
        assert!(CurrentThreadTaskRuntime::with_max_blocking_threads(0).is_err());
        let runtime = CurrentThreadTaskRuntime::with_max_blocking_threads(1).unwrap();
        runtime.block_on(async {
            let value = std::rc::Rc::new(17);
            let task = spawn_local_detached("local-runtime-test", async move {
                tokio::task::yield_now().await;
                *value
            })
            .unwrap();
            assert_eq!(task.await.unwrap(), Some(17));
        });
    }

    #[test]
    fn block_on_helpers_run_to_completion() {
        assert_eq!(block_on_tokio_current_thread(async { 9 }).unwrap(), 9);
        assert_eq!(block_on_async_current(async { 10 }), 10);
    }

    #[test]
    fn has_tokio_runtime_reflects_context() {
        assert!(!has_tokio_runtime());
        let runtime = build_current_thread_runtime().unwrap();
        runtime.block_on(async { assert!(has_tokio_runtime()) });
    }

    #[test]
    fn wait_for_shutdown_signal_returns_immediately_when_notified() {
        let (notifier, _waiter) = crate::imp::sync::async_::notify_wait();
        notifier.notify_all();
        wait_for_shutdown_signal(notifier);
    }
}
