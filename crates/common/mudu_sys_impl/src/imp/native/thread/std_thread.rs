use crate::imp::native::thread::context::{try_this_thread_task_id, ThreadTaskContextGuard};
use crate::imp::task::id::TaskID;
use crate::imp::task::SJoinHandle;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use std::time::Duration;

/// Namespace for blocking thread operations over `std::thread`.
pub struct Thread;

/// Join handle of a thread spawned through [`Thread`].
pub struct ThreadJoinHandle<T>(std::thread::JoinHandle<T>);

impl<T> ThreadJoinHandle<T> {
    /// Wraps a [`std::thread::JoinHandle`].
    pub fn new(inner: std::thread::JoinHandle<T>) -> Self {
        Self(inner)
    }

    /// Waits for the thread to finish and returns its result, or the panic
    /// payload if the thread panicked.
    pub fn join(self) -> std::thread::Result<T> {
        self.0.join()
    }

    /// Returns `true` if the thread has finished running.
    pub fn is_finished(&self) -> bool {
        self.0.is_finished()
    }
}

impl Thread {
    /// Blocks the current thread for at least `dur`.
    pub fn sleep(dur: Duration) {
        std::thread::sleep(dur);
    }

    /// Spawns a thread named "thread" running `f`.
    pub fn spawn<F, T>(f: F) -> RS<ThreadJoinHandle<T>>
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static,
    {
        Self::spawn_named("thread", f)
    }

    /// Spawns a named thread running `f`.
    ///
    /// The thread is registered in the task context registry for tracing
    /// before `f` runs and deregistered when it returns.
    pub fn spawn_named<F, T>(name: impl Into<String>, f: F) -> RS<ThreadJoinHandle<T>>
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static,
    {
        let name = name.into();
        let task_name = name.clone();
        let parent = try_this_thread_task_id();
        std::thread::Builder::new()
            .name(name)
            .spawn(move || {
                let _task_ctx = ThreadTaskContextGuard::enter(task_name, parent);
                f()
            })
            .map(ThreadJoinHandle::new)
            .map_err(|e| mudu_error!(ErrorCode::Thread, "spawn thread error", e))
    }

    /// Spawns a named thread running `f`, prescribing the thread's task id
    /// instead of generating a random one.
    ///
    /// Used to align a thread's tracing identity with an identity allocated
    /// elsewhere (e.g. a worker id doubling as its channel endpoint id). The
    /// caller owns the id's uniqueness in the task context registry.
    pub fn spawn_named_with_id<F, T>(
        name: impl Into<String>,
        id: TaskID,
        f: F,
    ) -> RS<ThreadJoinHandle<T>>
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static,
    {
        let name = name.into();
        let task_name = name.clone();
        let parent = try_this_thread_task_id();
        std::thread::Builder::new()
            .name(name)
            .spawn(move || {
                let _task_ctx = ThreadTaskContextGuard::enter_with_id(task_name, id, parent);
                f()
            })
            .map(ThreadJoinHandle::new)
            .map_err(|e| mudu_error!(ErrorCode::Thread, "spawn thread error", e))
    }
}

/// Blocks the current thread for at least `dur`.
pub fn sleep(dur: Duration) {
    Thread::sleep(dur);
}

/// Spawns a thread running `f` and returns a join handle for its result.
pub fn spawn_thread<F, T>(f: F) -> RS<SJoinHandle<T>>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    Thread::spawn(f).map(SJoinHandle::new)
}

/// Spawns a named thread running `f` and returns a join handle for its
/// result.
pub fn spawn_thread_named<F, T>(name: impl Into<String>, f: F) -> RS<SJoinHandle<T>>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    Thread::spawn_named(name, f).map(SJoinHandle::new)
}

/// Spawns a named thread with a caller-prescribed task id and returns a
/// join handle for its result. See [`Thread::spawn_named_with_id`].
pub fn spawn_thread_named_with_id<F, T>(
    name: impl Into<String>,
    id: TaskID,
    f: F,
) -> RS<SJoinHandle<T>>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    Thread::spawn_named_with_id(name, id, f).map(SJoinHandle::new)
}
