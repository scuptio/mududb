use crate::sync::async_::Waiter;
use mudu::common::result::RS;
use std::future::Future;
use std::time::Duration;
use tokio::task::JoinHandle;

pub fn spawn_local_task<F>(
    cancel_waiter: Waiter,
    name: &str,
    future: F,
) -> RS<JoinHandle<Option<F::Output>>>
where
    F: Future + 'static,
    F::Output: 'static,
{
    crate::imp::task::spawn_local_task(cancel_waiter, name, future)
}

pub fn spawn_local_detached<F>(name: &str, future: F) -> RS<JoinHandle<Option<F::Output>>>
where
    F: Future + 'static,
    F::Output: 'static,
{
    crate::imp::task::spawn_local_detached(name, future)
}

pub use crate::imp::spawn_local::TaskFailed;
pub use crate::imp::task::LocalTaskSet;

pub async fn __select_local_till_done_or_timeout<F>(
    waiter: Waiter,
    duration: Duration,
    future: F,
) -> Result<F::Output, TaskFailed>
where
    F: Future + 'static,
    F::Output: 'static,
{
    crate::imp::spawn_local::__select_local_till_done_or_timeout(waiter, duration, future).await
}
