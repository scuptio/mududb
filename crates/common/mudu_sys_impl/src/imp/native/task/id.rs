use crate::imp::random::Uuid;
use std::cell::Cell;
use tokio::task_local;

/// Unique id of a task or thread, used as the key of the task context
/// registry.
pub type TaskID = u128;

/// Generates a fresh task id from a random UUID v4.
pub fn new_task_id() -> TaskID {
    Uuid::new_v4().as_u128()
}

task_local! {
    /// Task id of the currently executing Tokio task (set by `spawn_task`
    /// and friends).
    pub static TASK_ID: TaskID;
}

thread_local! {
    static CURRENT_POLL_TASK_ID: Cell<Option<TaskID>> = const { Cell::new(None) };
}

/// RAII guard that records the task id currently being polled on this
/// thread; restores the previous value on drop.
///
/// Used by custom executors (the io_uring worker loop) where Tokio
/// task-locals do not apply.
pub struct PollTaskIdGuard(Option<TaskID>);

impl Drop for PollTaskIdGuard {
    fn drop(&mut self) {
        CURRENT_POLL_TASK_ID.with(|f| f.set(self.0));
    }
}

impl PollTaskIdGuard {
    /// Sets the poll task id for the current thread until the guard drops.
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
