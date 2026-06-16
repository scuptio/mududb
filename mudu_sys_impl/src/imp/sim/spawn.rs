use crate::sync::async_::Waiter;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use std::future::Future;
use tokio::task::JoinHandle;

pub fn spawn_task<F>(
    _cancel_waiter: Waiter,
    _name: &str,
    _future: F,
) -> RS<JoinHandle<Option<F::Output>>>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    Err(m_error!(EC::NotImplemented, "[sim] spawn_task"))
}

pub fn spawn_task_detached<F>(_name: &str, _future: F) -> RS<JoinHandle<Option<F::Output>>>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    Err(m_error!(EC::NotImplemented, "[sim] spawn_task_detached"))
}

pub async fn spawn_blocking<F, T>(_f: F) -> RS<T>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    Err(m_error!(EC::NotImplemented, "[sim] spawn_blocking"))
}
