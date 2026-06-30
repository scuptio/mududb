use crate::imp::task::id::{new_task_id, TaskID};
use crate::imp::task::TaskContext;
use std::backtrace::Backtrace;
use std::cell::Cell;

thread_local! {
    static THREAD_TASK_ID: Cell<Option<TaskID>> = const { Cell::new(None) };
}

pub fn try_this_thread_task_id() -> Option<TaskID> {
    THREAD_TASK_ID.with(Cell::get)
}

pub(super) struct ThreadTaskContextGuard {
    id: TaskID,
}

impl ThreadTaskContextGuard {
    pub(super) fn enter(name: String) -> Self {
        let id = new_task_id();
        let ctx = TaskContext::new_thread_context(id, name);
        ctx.watch("state", "running");
        ctx.enter_thread(Backtrace::force_capture().to_string());
        THREAD_TASK_ID.with(|slot| slot.set(Some(id)));
        Self { id }
    }
}

impl Drop for ThreadTaskContextGuard {
    fn drop(&mut self) {
        THREAD_TASK_ID.with(|slot| slot.set(None));
        TaskContext::remove_context(self.id);
    }
}
