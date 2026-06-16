use crate::task::sync::SJoinHandle;
use crate::task::context::TaskContext;
use crate::task::id::{TaskID, new_task_id};
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use std::cell::Cell;
use std::backtrace::Backtrace;
use std::time::Duration;

thread_local! {
    static THREAD_TASK_ID: Cell<Option<TaskID>> = const { Cell::new(None) };
}

pub fn try_this_thread_task_id() -> Option<TaskID> {
    THREAD_TASK_ID.with(Cell::get)
}

struct ThreadTaskContextGuard {
    id: TaskID,
}

impl ThreadTaskContextGuard {
    fn enter(name: String) -> Self {
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

pub fn sleep(dur: Duration) {
    std::thread::sleep(dur);
}

pub fn spawn_thread<F, T>(f: F) -> RS<SJoinHandle<T>>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    spawn_thread_named("thread", f)
}

pub fn spawn_thread_named<F, T>(name: impl Into<String>, f: F) -> RS<SJoinHandle<T>>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    let name = name.into();
    let task_name = name.clone();
    std::thread::Builder::new()
        .name(name)
        .spawn(move || {
            let _task_ctx = ThreadTaskContextGuard::enter(task_name);
            f()
        })
        .map(SJoinHandle::new)
        .map_err(|e| m_error!(EC::ThreadErr, "spawn thread error", e))
}
