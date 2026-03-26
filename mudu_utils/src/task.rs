use std::future::Future;

use std::time::Duration;

use crate::notifier::NotifyWait;
use crate::task_context::TaskContext;
use crate::task_id;
use crate::task_id::TaskID;
use mudu::common::result::RS;
use tokio::task::JoinHandle;
use tokio::time::sleep;
use tokio::{select, task, task_local};
use tracing::trace;

task_local! {
    static TASK_ID: TaskID;
}

/// The task must create by `task::spawn_local_task`, or `task::spawn_task` to set `TASK_ID` value.
/// if not, the `LocalKey::get` would raise such panic,
///     "cannot access a task-local storage value without setting it first"
pub fn this_task_id() -> TaskID {
    TASK_ID.get()
}

#[macro_export]
macro_rules! task_trace {
    () => {{
        #[cfg(feature = "debug_trace")]
        {
            let s = async_backtrace::location!();
            $crate::task_trace::TaskTrace::new(s)
        }
        #[cfg(not(feature = "debug_trace"))]
        {
            $crate::task_trace::NoopTaskTrace::new()
        }
    }};
}

#[macro_export]
macro_rules! dump_task_trace {
    () => {{
        #[cfg(feature = "debug_trace")]
        {
            $crate::task_trace::TaskTrace::dump_task_trace()
        }
        #[cfg(not(feature = "debug_trace"))]
        {
            String::new()
        }
    }};
}

#[macro_export]
macro_rules! task_backtrace {
    () => {{
        #[cfg(feature = "debug_trace")]
        {
            $crate::task_trace::TaskTrace::backtrace()
        }
        #[cfg(not(feature = "debug_trace"))]
        {
            String::new()
        }
    }};
}

#[macro_export]
macro_rules! this_task_id {
    () => {{ $crate::task_trace::this_task_id() }};
}

pub fn spawn_local_task<F>(
    cancel_notifier: NotifyWait,
    _name: &str,
    future: F,
) -> RS<JoinHandle<Option<F::Output>>>
where
    F: Future + 'static,
    F::Output: 'static,
{
    let id = task_id::new_task_id();
    let _ = TaskContext::new_context(id, _name.to_string(), false);
    Ok(task::spawn_local(TASK_ID.scope(id, async move {
        let r = __select_local_till_done(cancel_notifier, future).await;
        TaskContext::remove_context(id);
        r
    })))
}

pub fn spawn_task<F>(
    cancel_notifier: NotifyWait,
    _name: &str,
    future: F,
) -> RS<JoinHandle<Option<F::Output>>>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    let id = task_id::new_task_id();
    let _ = TaskContext::new_context(id, _name.to_string(), false);
    Ok(task::spawn(TASK_ID.scope(id, async move {
        let r = __select_till_done(cancel_notifier, future).await;
        TaskContext::remove_context(id);
        r
    })))
}

pub fn spawn_local_task_timeout<F>(
    cancel_notifier: NotifyWait,
    duration: Duration,
    _name: &str,
    future: F,
) -> RS<JoinHandle<Result<F::Output, TaskFailed>>>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    Ok(task::spawn_local(async move {
        __select_local_till_done_or_timeout(cancel_notifier, duration, future).await
    }))
}

async fn __select_local_till_done<F>(notify: NotifyWait, future: F) -> Option<F::Output>
where
    F: Future + 'static,
    F::Output: 'static,
{
    let future = async move {
        let r = select! {
            _ = notify.notified() => {
                trace ! ("local task stop");
                None
            }
            r = future => {
                trace ! ("local task  end");
                Some(r)
            }
        };
        r
    };
    future.await
}

pub enum TaskFailed {
    Cancel,
    Timeout,
}

async fn __select_local_till_done_or_timeout<F>(
    notify: NotifyWait,
    duration: Duration,
    future: F,
) -> Result<F::Output, TaskFailed>
where
    F: Future + 'static,
    F::Output: 'static,
{
    let future = async move {
        let r = select! {
            _ = notify.notified() => {
                trace ! ("local task stop");
                 Err(TaskFailed::Cancel)
            }
            r = future => {
                trace ! ("local task  end");
                Ok(r)
            }
            _ = sleep(duration) => {
                Err(TaskFailed::Timeout)
            }
        };
        r
    };
    future.await
}

async fn __select_till_done<F>(notify: NotifyWait, future: F) -> Option<F::Output>
where
    F: Future + 'static,
    F::Output: Send + 'static,
{
    let future = async move {
        let r = select! {
            _ = notify.notified() => {
                trace ! ("task stop");
                None
            }
            r = future => {
                trace ! ("task  end");
                Some(r)
            }
        };
        r
    };
    future.await
}
