use crate::imp::thread::ThreadJoinHandle;
use mudu::common::result::RS;
use std::time::Duration;

pub struct TaskSync;

impl TaskSync {
    pub fn sleep_blocking(dur: Duration) {
        std::thread::sleep(dur);
    }
}

pub struct SJoinHandle<T>(Inner<T>);

enum Inner<T> {
    Real(ThreadJoinHandle<T>),
    Done(std::thread::Result<T>),
}

impl<T> SJoinHandle<T> {
    pub fn new(inner: ThreadJoinHandle<T>) -> Self {
        Self(Inner::Real(inner))
    }

    pub fn join(self) -> std::thread::Result<T> {
        match self.0 {
            Inner::Real(handle) => handle.join(),
            Inner::Done(result) => result,
        }
    }

    pub fn is_finished(&self) -> bool {
        match &self.0 {
            Inner::Real(handle) => handle.is_finished(),
            Inner::Done(_) => true,
        }
    }

    pub fn new_mock(result: T) -> Self
    where
        T: Send + 'static,
    {
        Self(Inner::Done(Ok(result)))
    }
}

pub fn sleep_blocking(dur: Duration) {
    crate::imp::thread::sleep(dur)
}

pub fn spawn_thread<F, T>(f: F) -> RS<SJoinHandle<T>>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    crate::imp::thread::spawn_thread(f)
}

pub fn spawn_thread_named<F, T>(name: impl Into<String>, f: F) -> RS<SJoinHandle<T>>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    crate::imp::thread::spawn_thread_named(name, f)
}

pub fn try_this_thread_task_id() -> Option<super::id::TaskID> {
    crate::imp::thread::try_this_thread_task_id()
}
