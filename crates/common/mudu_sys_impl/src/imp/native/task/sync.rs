use crate::imp::thread::ThreadJoinHandle;
use mudu::common::result::RS;
use std::time::Duration;

/// Namespace for blocking thread operations.
pub struct TaskSync;

impl TaskSync {
    /// Blocks the current thread for at least `dur`.
    pub fn sleep_blocking(dur: Duration) {
        std::thread::sleep(dur);
    }
}

/// Join handle of a thread or a finished result.
///
/// `new_mock` creates an already-finished handle, which lets test doubles
/// return a handle without spawning a thread.
pub struct SJoinHandle<T>(Inner<T>);

enum Inner<T> {
    Real(ThreadJoinHandle<T>),
    Done(std::thread::Result<T>),
}

impl<T> SJoinHandle<T> {
    /// Wraps a real thread join handle.
    pub fn new(inner: ThreadJoinHandle<T>) -> Self {
        Self(Inner::Real(inner))
    }

    /// Waits for the thread to finish and returns its result, or the panic
    /// payload if the thread panicked.
    pub fn join(self) -> std::thread::Result<T> {
        match self.0 {
            Inner::Real(handle) => handle.join(),
            Inner::Done(result) => result,
        }
    }

    /// Returns `true` if the thread has finished running (always `true` for
    /// mock handles).
    pub fn is_finished(&self) -> bool {
        match &self.0 {
            Inner::Real(handle) => handle.is_finished(),
            Inner::Done(_) => true,
        }
    }

    /// Creates an already-finished handle yielding `result`.
    pub fn new_mock(result: T) -> Self
    where
        T: Send + 'static,
    {
        Self(Inner::Done(Ok(result)))
    }
}

/// Blocks the current thread for at least `dur`.
pub fn sleep_blocking(dur: Duration) {
    crate::imp::thread::sleep(dur)
}

/// Spawns a thread running `f` and returns a join handle for its result.
pub fn spawn_thread<F, T>(f: F) -> RS<SJoinHandle<T>>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    crate::imp::thread::spawn_thread(f)
}

/// Spawns a named thread running `f` and returns a join handle for its
/// result.
pub fn spawn_thread_named<F, T>(name: impl Into<String>, f: F) -> RS<SJoinHandle<T>>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    crate::imp::thread::spawn_thread_named(name, f)
}

/// Returns the task id assigned to the current thread, or `None` if the
/// current thread was not spawned through this crate.
pub fn try_this_thread_task_id() -> Option<super::id::TaskID> {
    crate::imp::thread::try_this_thread_task_id()
}
