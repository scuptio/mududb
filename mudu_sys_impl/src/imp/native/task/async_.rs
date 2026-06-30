use super::id::TaskID;
use mudu::common::result::RS;
use std::cell::Cell;
use std::future::Future;
use std::time::Duration;
use tokio::task_local;

task_local! {
    pub static TASK_ID: TaskID;
}

thread_local! {
    static CURRENT_POLL_TASK_ID: Cell<Option<TaskID>> = const { Cell::new(None) };
}

pub struct PollTaskIdGuard(Option<TaskID>);

impl Drop for PollTaskIdGuard {
    fn drop(&mut self) {
        CURRENT_POLL_TASK_ID.with(|f| f.set(self.0));
    }
}

impl PollTaskIdGuard {
    pub fn enter(task_id: TaskID) -> PollTaskIdGuard {
        PollTaskIdGuard(CURRENT_POLL_TASK_ID.with(|f| f.replace(Some(task_id))))
    }
}

/// 获取当前任务的ID（如果存在）
pub fn try_this_task_id() -> Option<TaskID> {
    TASK_ID.try_with(|f| *f).ok()
}

/// 获取当前任务的ID（必须存在，否则 panic）
#[expect(
    clippy::expect_used,
    reason = "this_task_id is only valid inside a task context; callers should use try_this_task_id for optional access"
)]
pub fn this_task_id() -> TaskID {
    try_this_task_id()
        .expect("cannot access task id: neither tokio task-local nor poll-task TLS is set")
}

/// 获取当前正在poll的任务ID（用于跨线程/LocalSet场景）
pub fn current_poll_task_id() -> Option<TaskID> {
    CURRENT_POLL_TASK_ID.with(|f| f.get())
}

pub struct TaskAsync;

impl TaskAsync {
    pub async fn sleep(dur: Duration) -> RS<()> {
        tokio::time::sleep(dur).await;
        Ok(())
    }

    pub async fn timeout<F>(dur: Duration, fut: F) -> Option<F::Output>
    where
        F: Future,
    {
        tokio::time::timeout(dur, fut).await.ok()
    }
}

pub async fn sleep(dur: Duration) -> RS<()> {
    TaskAsync::sleep(dur).await
}

pub async fn timeout<F>(dur: Duration, fut: F) -> Option<F::Output>
where
    F: Future,
{
    TaskAsync::timeout(dur, fut).await
}

pub use super::join_handle::{TaskJoinError, TaskJoinHandle};
pub use super::runtime::{
    block_on_async_current, block_on_tokio_current_thread, build_current_thread_runtime,
    build_multi_thread_runtime, has_tokio_runtime, wait_for_shutdown_signal,
    CurrentThreadTaskRuntime, TaskRuntime, TaskRuntimeEnterGuard,
};
pub use super::spawn::{spawn_blocking, spawn_task, spawn_task_detached};
pub use super::spawn_local::{spawn_local_detached, spawn_local_task, LocalTaskSet, TaskFailed};
