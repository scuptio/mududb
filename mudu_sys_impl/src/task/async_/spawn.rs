use crate::sync::async_::Waiter;
use mudu::common::result::RS;
use std::future::Future;
use tokio::task::JoinHandle;

pub fn spawn_task<F>(
    cancel_waiter: Waiter,
    name: &str,
    future: F,
) -> RS<JoinHandle<Option<F::Output>>>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    crate::imp::task::spawn_task(cancel_waiter, name, future)
}

pub fn spawn_task_detached<F>(name: &str, future: F) -> RS<JoinHandle<Option<F::Output>>>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    crate::imp::task::spawn_task_detached(name, future)
}

pub async fn spawn_blocking<F, T>(f: F) -> RS<T>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    crate::imp::task::spawn_blocking(f).await
}
