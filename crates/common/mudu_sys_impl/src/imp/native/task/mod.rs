/// Async task spawning, runtime builders, and task-id tracking.
pub mod async_;
/// Per-task tracing context registry.
pub mod context;
/// Task id type and generator.
pub mod id;
mod join_handle;
mod runtime;
mod spawn;
mod spawn_local;
/// Blocking thread-based task helpers.
pub mod sync;
/// Task tracing (async backtrace watch points).
pub mod trace;

pub use async_::{
    block_on_async_current, block_on_tokio_current_thread, build_current_thread_runtime,
    build_multi_thread_runtime, current_poll_task_id, has_tokio_runtime, sleep, spawn_blocking,
    spawn_local_detached, spawn_local_task, spawn_task, spawn_task_detached, this_task_id, timeout,
    try_this_task_id, wait_for_shutdown_signal, CurrentThreadTaskRuntime, LocalTaskSet,
    PollTaskIdGuard, TaskAsync, TaskFailed, TaskJoinError, TaskJoinHandle, TaskRuntime,
    TaskRuntimeEnterGuard,
};
pub use context::TaskContext;
pub use id::{new_task_id, TaskID};
pub use sync::{
    sleep_blocking, spawn_thread, spawn_thread_named, try_this_thread_task_id, SJoinHandle,
    TaskSync,
};
pub use trace::{NoopTaskTrace, TaskTrace};

use crate::imp::sync::async_::Waiter;
use mudu::common::result::RS;
use std::future::Future;

/// Async task subsystem - native implementation over Tokio.
#[derive(Default)]
pub struct SysTasks;

impl SysTasks {
    /// Creates the async task subsystem handle.
    pub fn new() -> Self {
        Self
    }

    /// Spawns `future` as a named background task with a fresh cancellation
    /// notifier; the join handle resolves to `None` if cancelled.
    pub fn spawn<F>(&self, name: &str, future: F) -> RS<TaskJoinHandle<Option<F::Output>>>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        let (_cancel_notifier, cancel_waiter) = crate::imp::sync::async_::notify_wait();
        self.spawn_with_waiter(cancel_waiter, name, future)
    }

    /// Spawns `future` as a named background task cancelled via
    /// `cancel_waiter`.
    pub fn spawn_with_waiter<F>(
        &self,
        cancel_waiter: Waiter,
        name: &str,
        future: F,
    ) -> RS<TaskJoinHandle<Option<F::Output>>>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        spawn_task(cancel_waiter, name, future)
    }

    /// Spawns a `!Send` future on the current thread's local task set.
    pub fn spawn_local<F>(&self, name: &str, future: F) -> RS<TaskJoinHandle<Option<F::Output>>>
    where
        F: Future + 'static,
        F::Output: 'static,
    {
        let (_cancel_notifier, cancel_waiter) = crate::imp::sync::async_::notify_wait();
        self.spawn_local_with_waiter(cancel_waiter, name, future)
    }

    /// Spawns a `!Send` future on the current thread's local task set,
    /// cancelled via `cancel_waiter`.
    pub fn spawn_local_with_waiter<F>(
        &self,
        cancel_waiter: Waiter,
        name: &str,
        future: F,
    ) -> RS<TaskJoinHandle<Option<F::Output>>>
    where
        F: Future + 'static,
        F::Output: 'static,
    {
        spawn_local_task(cancel_waiter, name, future)
    }

    /// Runs the blocking closure `f` on the blocking thread pool and awaits
    /// its result.
    pub async fn spawn_blocking<F, R>(&self, f: F) -> RS<R>
    where
        F: FnOnce() -> R + Send + 'static,
        R: Send + 'static,
    {
        spawn_blocking(f).await
    }

    /// Builds a new current-thread Tokio runtime.
    pub fn build_current_thread_runtime(&self) -> RS<TaskRuntime> {
        build_current_thread_runtime()
    }

    /// Builds a new multi-thread Tokio runtime.
    pub fn build_multi_thread_runtime(&self) -> RS<TaskRuntime> {
        build_multi_thread_runtime()
    }

    /// Returns `true` if called from within a Tokio runtime context.
    pub fn has_tokio_runtime(&self) -> bool {
        has_tokio_runtime()
    }

    /// Blocks until Ctrl-C/SIGTERM (Unix) or Ctrl-C (other platforms) is
    /// received or `stop` is notified, then notifies `stop` so dependents
    /// shut down.
    pub fn wait_for_shutdown_signal(&self, stop: crate::imp::sync::async_::Notifier) {
        wait_for_shutdown_signal(stop)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn systasks_new_and_default() {
        let _ = SysTasks::new();
        let _ = SysTasks;
    }

    #[test]
    fn systasks_build_current_thread_runtime() {
        let tasks = SysTasks::new();
        let rt = tasks.build_current_thread_runtime().unwrap();
        let result = rt.block_on(async { 42 });
        assert_eq!(result, 42);
    }

    #[test]
    fn systasks_build_multi_thread_runtime() {
        let tasks = SysTasks::new();
        let rt = tasks.build_multi_thread_runtime().unwrap();
        let result = rt.block_on(async { 21 });
        assert_eq!(result, 21);
    }

    #[test]
    fn systasks_has_tokio_runtime_false_outside() {
        let tasks = SysTasks::new();
        assert!(!tasks.has_tokio_runtime());
    }

    #[test]
    fn systasks_has_tokio_runtime_true_inside() {
        let tasks = SysTasks::new();
        let rt = tasks.build_current_thread_runtime().unwrap();
        let inside = rt.block_on(async { tasks.has_tokio_runtime() });
        assert!(inside);
    }

    #[test]
    fn systasks_spawn_blocking() {
        let tasks = SysTasks::new();
        let rt = tasks.build_current_thread_runtime().unwrap();
        let result = rt.block_on(async { tasks.spawn_blocking(|| 42).await.unwrap() });
        assert_eq!(result, 42);
    }

    #[test]
    fn systasks_spawn_and_await() {
        let tasks = SysTasks::new();
        let rt = tasks.build_current_thread_runtime().unwrap();
        let result = rt.block_on(async {
            let handle = tasks.spawn("spawn", async { 7 }).unwrap();
            handle.await.unwrap()
        });
        assert_eq!(result, Some(7));
    }

    #[test]
    fn systasks_spawn_local_and_await() {
        let tasks = SysTasks::new();
        let rt = CurrentThreadTaskRuntime::new().unwrap();
        let result = rt.block_on(async move {
            let handle = tasks.spawn_local("local", async { 7 }).unwrap();
            handle.await.unwrap()
        });
        assert_eq!(result, Some(7));
    }
}
