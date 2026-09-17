use crate::imp::native::thread::context::ThreadTaskContextGuard;
use crate::imp::task::SJoinHandle;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use std::time::Duration;

pub struct Thread;

pub struct ThreadJoinHandle<T>(std::thread::JoinHandle<T>);

impl<T> ThreadJoinHandle<T> {
    pub fn new(inner: std::thread::JoinHandle<T>) -> Self {
        Self(inner)
    }

    pub fn join(self) -> std::thread::Result<T> {
        self.0.join()
    }

    pub fn is_finished(&self) -> bool {
        self.0.is_finished()
    }
}

impl Thread {
    pub fn sleep(dur: Duration) {
        std::thread::sleep(dur);
    }

    pub fn spawn<F, T>(f: F) -> RS<ThreadJoinHandle<T>>
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static,
    {
        Self::spawn_named("thread", f)
    }

    pub fn spawn_named<F, T>(name: impl Into<String>, f: F) -> RS<ThreadJoinHandle<T>>
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
            .map(ThreadJoinHandle::new)
            .map_err(|e| mudu_error!(ErrorCode::Thread, "spawn thread error", e))
    }
}

pub fn sleep(dur: Duration) {
    Thread::sleep(dur);
}

pub fn spawn_thread<F, T>(f: F) -> RS<SJoinHandle<T>>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    Thread::spawn(f).map(SJoinHandle::new)
}

pub fn spawn_thread_named<F, T>(name: impl Into<String>, f: F) -> RS<SJoinHandle<T>>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    Thread::spawn_named(name, f).map(SJoinHandle::new)
}
