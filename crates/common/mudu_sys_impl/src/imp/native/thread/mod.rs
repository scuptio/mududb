use crate::imp::task::SJoinHandle;
use mudu::common::result::RS;

mod context;
mod std_thread;

pub use context::try_this_thread_task_id;
pub use std_thread::{
    sleep, spawn_thread, spawn_thread_named, spawn_thread_named_with_id, Thread, ThreadJoinHandle,
};

/// Thread subsystem - native implementation over `std::thread`.
#[derive(Default)]
pub struct SysThread;

impl SysThread {
    /// Creates the thread subsystem handle.
    pub fn new() -> Self {
        Self
    }

    /// Spawns a thread running `f` and returns a join handle for its result.
    pub fn spawn<F, T>(&self, f: F) -> RS<SJoinHandle<T>>
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static,
    {
        spawn_thread(f)
    }

    /// Spawns a named thread running `f` and returns a join handle for its
    /// result.
    pub fn spawn_named<F, T>(&self, name: impl Into<String>, f: F) -> RS<SJoinHandle<T>>
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static,
    {
        spawn_thread_named(name, f)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn systhread_new_and_default() {
        let _ = SysThread::new();
        let _ = SysThread;
    }

    #[test]
    fn systhread_spawn() {
        let thread = SysThread::new();
        let handle = thread.spawn(|| 42).unwrap();
        assert_eq!(handle.join().unwrap(), 42);
    }

    #[test]
    fn systhread_spawn_named() {
        let thread = SysThread::new();
        let handle = thread
            .spawn_named("my-thread", || {
                std::thread::current().name().map(|s| s.to_string())
            })
            .unwrap();
        let name = handle.join().unwrap().unwrap();
        assert!(name.contains("my-thread"));
    }

    #[test]
    fn systhread_spawn_named_returns_join_handle() {
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;

        let thread = SysThread::new();
        let can_finish = Arc::new(AtomicBool::new(false));
        let can_finish_clone = can_finish.clone();
        let handle = thread
            .spawn_named("named-handle", move || {
                while !can_finish_clone.load(Ordering::Relaxed) {
                    std::thread::yield_now();
                }
                123
            })
            .unwrap();
        let finished_before = handle.is_finished();
        can_finish.store(true, Ordering::Relaxed);
        let result = handle.join().unwrap();
        assert!(!finished_before);
        assert_eq!(result, 123);
    }

    #[test]
    fn spawn_named_with_id_prescribes_the_thread_task_id() {
        use crate::imp::task::TaskContext;

        let id = 0xABCD_EF01_2345_6789_0ABC_DEF1_2345_6789u128;
        let handle = spawn_thread_named_with_id("prescribed", id, move || {
            let tls_id = try_this_thread_task_id();
            let registered = TaskContext::get(id).is_some();
            (tls_id, registered)
        })
        .unwrap();
        let (tls_id, registered) = handle.join().unwrap();
        assert_eq!(tls_id, Some(id));
        assert!(registered);
        // The context is deregistered when the thread exits.
        assert!(TaskContext::get(id).is_none());
    }

    #[test]
    fn spawned_thread_and_task_record_the_spawner_as_parent() {
        use crate::imp::task::id::new_task_id;
        use crate::imp::task::TaskContext;

        let handle = spawn_thread_named("parent-thread", move || {
            let thread_id = try_this_thread_task_id().unwrap();
            // A task context created on this thread names it as parent.
            let task_ctx = TaskContext::new_tokio_context(new_task_id(), "child-task".to_string());
            let task_parent = task_ctx.parent_thread();
            TaskContext::remove_context(task_ctx.id());
            // A nested thread gets this thread's id as its parent.
            let nested = spawn_thread_named("nested-thread", move || {
                TaskContext::get(try_this_thread_task_id().unwrap()).map(|ctx| ctx.parent_thread())
            })
            .unwrap();
            (thread_id, task_parent, nested.join().unwrap())
        })
        .unwrap();
        let (thread_id, task_parent, nested_parent) = handle.join().unwrap();
        assert_eq!(task_parent, Some(thread_id));
        assert_eq!(nested_parent, Some(Some(thread_id)));
    }
}
