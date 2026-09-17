//! Worker task registry and async function task waker.
#![allow(missing_docs)]
pub(crate) mod async_func_task_waker;
#[cfg(target_os = "linux")]
pub(crate) mod task_registry;
#[cfg(target_os = "linux")]
pub mod worker_task;
