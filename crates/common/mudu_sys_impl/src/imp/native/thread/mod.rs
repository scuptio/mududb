#![allow(missing_docs)]
use crate::imp::task::SJoinHandle;
use mudu::common::result::RS;

mod context;
mod std_thread;

pub use context::try_this_thread_task_id;
pub use std_thread::{sleep, spawn_thread, spawn_thread_named, Thread, ThreadJoinHandle};

#[derive(Default)]
pub struct SysThread;

impl SysThread {
    pub fn new() -> Self {
        Self
    }

    pub fn spawn<F, T>(&self, f: F) -> RS<SJoinHandle<T>>
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static,
    {
        spawn_thread(f)
    }

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
}
