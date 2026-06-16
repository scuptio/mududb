use crate::contract::async_mode::AsyncMode;
use crate::sync::async_::Notifier;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use std::future::Future;

pub enum Runtime {
    Sim,
}

impl Runtime {
    pub fn new() -> Self {
        Self::Sim
    }

    pub fn mode(&self) -> AsyncMode {
        AsyncMode::Tokio
    }
}

impl Default for Runtime {
    fn default() -> Self {
        Self::new()
    }
}

pub fn build_current_thread_runtime() -> RS<Runtime> {
    Err(m_error!(
        EC::NotImplemented,
        "[sim] build_current_thread_runtime"
    ))
}

pub fn build_multi_thread_runtime() -> RS<Runtime> {
    Err(m_error!(
        EC::NotImplemented,
        "[sim] build_multi_thread_runtime"
    ))
}

pub struct CurrentThreadTaskRuntime;

impl CurrentThreadTaskRuntime {
    pub fn new() -> RS<Self> {
        Err(m_error!(
            EC::NotImplemented,
            "[sim] CurrentThreadTaskRuntime::new"
        ))
    }
}

pub fn block_on_tokio_current_thread<F>(_fut: F) -> RS<F::Output>
where
    F: Future + 'static,
    F::Output: 'static,
{
    Err(m_error!(
        EC::NotImplemented,
        "[sim] block_on_tokio_current_thread"
    ))
}

pub fn block_on_async_current<F>(_fut: F) -> F::Output
where
    F: Future + 'static,
    F::Output: 'static,
{
    panic!("[sim] block_on_async_current not implemented")
}

pub fn wait_for_shutdown_signal(_stop: Notifier) {
    // sim: no signals to handle.
}

pub fn has_tokio_runtime() -> bool {
    false
}
