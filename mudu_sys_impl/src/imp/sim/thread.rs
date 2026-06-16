use crate::task::sync::SJoinHandle;
use crate::task::id::TaskID;
use mudu::common::result::RS;
use std::time::Duration;

pub fn sleep(_dur: Duration) {}

pub fn try_this_thread_task_id() -> Option<TaskID> {
    None
}

pub fn spawn_thread<F, T>(f: F) -> RS<SJoinHandle<T>>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    // sim: execute synchronously, no actual thread created
    let result = f();
    Ok(SJoinHandle::new_mock(result))
}

pub fn spawn_thread_named<F, T>(_name: impl Into<String>, f: F) -> RS<SJoinHandle<T>>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    // sim: name is ignored, execute synchronously
    spawn_thread(f)
}
