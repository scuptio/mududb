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

/// Spawns a named thread whose task id is prescribed by the caller (e.g. a
/// worker id, aligning the thread's tracing identity with its channel
/// endpoint id) and returns a join handle for its result.
pub fn spawn_thread_named_with_id<F, T>(
    name: impl Into<String>,
    id: super::id::TaskID,
    f: F,
) -> RS<SJoinHandle<T>>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    crate::imp::thread::spawn_thread_named_with_id(name, id, f)
}

/// Returns the task id assigned to the current thread, or `None` if the
/// current thread was not spawned through this crate.
pub fn try_this_thread_task_id() -> Option<super::id::TaskID> {
    crate::imp::thread::try_this_thread_task_id()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::panic)]
    use super::*;

    #[test]
    fn completed_handles_preserve_owned_results() {
        let handle = SJoinHandle::new_mock(String::from("completed"));
        assert!(handle.is_finished());
        assert_eq!(handle.join().unwrap(), "completed");
    }

    #[test]
    fn real_handles_wait_and_preserve_panic_payloads() {
        let (release, wait) = std::sync::mpsc::channel();
        let handle = spawn_thread(move || {
            wait.recv().unwrap();
            42
        })
        .unwrap();
        assert!(!handle.is_finished());
        release.send(()).unwrap();
        assert_eq!(handle.join().unwrap(), 42);
        let panic = spawn_thread(|| std::panic::panic_any(String::from("worker failed")))
            .unwrap()
            .join()
            .unwrap_err();
        assert_eq!(*panic.downcast::<String>().unwrap(), "worker failed");
    }

    #[test]
    fn named_threads_preserve_identity_and_remove_context() {
        let id = crate::imp::task::id::new_task_id();
        let handle = spawn_thread_named_with_id("sync-identity", id, move || {
            (
                try_this_thread_task_id(),
                std::thread::current().name().unwrap().to_owned(),
            )
        })
        .unwrap();
        let (actual_id, name) = handle.join().unwrap();
        assert_eq!(actual_id, Some(id));
        assert!(name.contains("sync-identity"));
        assert!(crate::imp::task::TaskContext::get(id).is_none());
        let named = spawn_thread_named("sync-generated", try_this_thread_task_id).unwrap();
        assert!(named.join().unwrap().is_some());
    }

    #[test]
    fn blocking_sleep_does_not_return_before_deadline() {
        let duration = Duration::from_millis(1);
        let start = std::time::Instant::now();
        TaskSync::sleep_blocking(duration);
        assert!(start.elapsed() >= duration);
        let start = std::time::Instant::now();
        sleep_blocking(duration);
        assert!(start.elapsed() >= duration);
    }
}
