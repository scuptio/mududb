use crate::imp::task::id::{new_task_id, TaskID};
use crate::imp::task::TaskContext;
use std::backtrace::Backtrace;
use std::cell::Cell;

thread_local! {
    static THREAD_TASK_ID: Cell<Option<TaskID>> = const { Cell::new(None) };
}

/// Returns the task id assigned to the current thread by
/// [`Thread::spawn_named`](crate::imp::thread::Thread), or `None` if the
/// current thread was not spawned through this crate.
pub fn try_this_thread_task_id() -> Option<TaskID> {
    THREAD_TASK_ID.with(Cell::get)
}

pub(super) struct ThreadTaskContextGuard {
    id: TaskID,
}

impl ThreadTaskContextGuard {
    pub(super) fn enter(name: String, parent: Option<TaskID>) -> Self {
        Self::enter_with_id(name, new_task_id(), parent)
    }

    /// Enters with a caller-prescribed task id instead of a random one,
    /// aligning the thread's tracing identity with an identity allocated
    /// elsewhere (e.g. a worker id doubling as its channel endpoint id).
    /// The caller owns the id's uniqueness in the task context registry.
    /// `parent` is the spawning thread's task id, captured on the spawning
    /// side (a fresh thread's TLS is empty).
    pub(super) fn enter_with_id(name: String, id: TaskID, parent: Option<TaskID>) -> Self {
        let ctx = TaskContext::new_thread_context_with_parent(id, name, parent);
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
