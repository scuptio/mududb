use super::{CURRENT_POLL_TASK_ID, TASK_ID};
use crate::task::id::TaskID;

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
pub fn this_task_id() -> TaskID {
    try_this_task_id()
        .expect("cannot access task id: neither tokio task-local nor poll-task TLS is set")
}

/// 获取当前正在poll的任务ID（用于跨线程/LocalSet场景）
pub fn current_poll_task_id() -> Option<TaskID> {
    CURRENT_POLL_TASK_ID.with(|f| f.get())
}
