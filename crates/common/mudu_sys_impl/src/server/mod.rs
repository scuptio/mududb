//! Worker task registry and async function task waker.
#[cfg(target_os = "linux")]
pub(crate) mod async_func_task_waker;
#[cfg(target_os = "linux")]
pub(crate) mod task_registry;
/// Async worker task unit driven by the worker task registry.
#[cfg(target_os = "linux")]
pub mod worker_task;
