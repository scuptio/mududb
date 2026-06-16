use crate::imp::env::Sys;
use crate::sync::async_::Notifier;
use mudu::common::result::RS;
use std::future::Future;
use tokio::runtime::Runtime;

pub fn build_current_thread_runtime() -> RS<Runtime> {
    Sys::build_current_thread_runtime()
}

pub fn build_multi_thread_runtime() -> RS<Runtime> {
    Sys::build_multi_thread_runtime()
}

pub use crate::imp::task_runtime::CurrentThreadTaskRuntime;

pub fn block_on_tokio_current_thread<F>(fut: F) -> RS<F::Output>
where
    F: Future + 'static,
    F::Output: 'static,
{
    crate::imp::task_runtime::block_on_tokio_current_thread(fut)
}

pub fn block_on_async_current<F>(fut: F) -> F::Output
where
    F: Future + 'static,
    F::Output: 'static,
{
    crate::imp::task_runtime::block_on_async_current(fut)
}

pub fn wait_for_shutdown_signal(stop: Notifier) {
    Sys::wait_for_shutdown_signal(stop)
}

pub fn has_tokio_runtime() -> bool {
    Sys::has_tokio_runtime()
}
