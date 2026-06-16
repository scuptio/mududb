use crate::sync::async_::Waiter;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use std::future::Future;
use std::time::Duration;
use tokio::runtime::Runtime;
use tokio::task::JoinHandle;

pub fn spawn_local_task<F>(
    _cancel_waiter: Waiter,
    _name: &str,
    _future: F,
) -> RS<JoinHandle<Option<F::Output>>>
where
    F: Future + 'static,
    F::Output: 'static,
{
    Err(m_error!(EC::NotImplemented, "[sim] spawn_local_task"))
}

pub fn spawn_local_detached<F>(_name: &str, _future: F) -> RS<JoinHandle<Option<F::Output>>>
where
    F: Future + 'static,
    F::Output: 'static,
{
    Err(m_error!(EC::NotImplemented, "[sim] spawn_local_detached"))
}

pub struct LocalTaskSet;

impl Default for LocalTaskSet {
    fn default() -> Self {
        Self::new()
    }
}

impl LocalTaskSet {
    pub fn new() -> Self {
        Self
    }

    pub fn spawn<F>(
        &self,
        _cancel_waiter: Waiter,
        _name: &str,
        _future: F,
    ) -> RS<JoinHandle<Option<F::Output>>>
    where
        F: Future + 'static,
        F::Output: 'static,
    {
        Err(m_error!(EC::NotImplemented, "[sim] LocalTaskSet::spawn"))
    }

    pub fn spawn_detached<F>(&self, _name: &str, _future: F) -> RS<JoinHandle<Option<F::Output>>>
    where
        F: Future + 'static,
        F::Output: 'static,
    {
        Err(m_error!(
            EC::NotImplemented,
            "[sim] LocalTaskSet::spawn_detached"
        ))
    }

    pub async fn run_until<F>(&self, future: F) -> F::Output
    where
        F: Future,
    {
        future.await
    }

    pub fn block_on<F>(&self, _runtime: &Runtime, _future: F) -> F::Output
    where
        F: Future,
    {
        // sim: no real runtime, just block on the future using a dummy approach.
        // This is a best-effort; real blocking requires a runtime.
        panic!("[sim] LocalTaskSet::block_on not implemented")
    }
}

pub async fn __select_local_till_done_or_timeout<F>(
    _waiter: Waiter,
    _duration: Duration,
    future: F,
) -> Result<F::Output, TaskFailed>
where
    F: Future + 'static,
    F::Output: 'static,
{
    Ok(future.await)
}

#[derive(Debug)]
pub enum TaskFailed {
    Cancel,
    Timeout,
}
