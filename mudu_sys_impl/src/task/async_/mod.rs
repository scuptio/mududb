use crate::task::id::TaskID;
use std::cell::Cell;
use tokio::task_local;

task_local! {
    pub static TASK_ID: TaskID;
}

thread_local! {
    static CURRENT_POLL_TASK_ID: Cell<Option<TaskID>> = const { Cell::new(None) };
}

pub mod id;
pub mod runtime;
pub mod spawn;
pub mod spawn_local;
pub mod util;

pub use id::{PollTaskIdGuard, current_poll_task_id, this_task_id, try_this_task_id};
pub use runtime::{
    CurrentThreadTaskRuntime, block_on_async_current, block_on_tokio_current_thread,
    build_current_thread_runtime, build_multi_thread_runtime, has_tokio_runtime,
    wait_for_shutdown_signal,
};
pub use spawn::{spawn_blocking, spawn_task, spawn_task_detached};
pub use spawn_local::{LocalTaskSet, spawn_local_detached, spawn_local_task};
pub use util::{TaskFailed, sleep, timeout};
