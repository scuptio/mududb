use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::task::id::TaskID;
use mudu::common::result::RS;

/// Boxed future driven by a [`WorkerTask`]; resolves to `Ok(())` on success.
pub type WorkerTaskFuture = Pin<Box<dyn Future<Output = RS<()>> + 'static>>;

/// A unit of async work owned by the worker task registry.
///
/// Each task carries the connection it belongs to (if any), a tracing task
/// id, and the future to poll. The `queued` and `completed` flags are shared
/// with the registry, and `waiting_on` records the in-flight IO operation
/// the task is currently blocked on.
pub struct WorkerTask {
    conn_id: Option<u64>,
    trace_task_id: TaskID,
    future: WorkerTaskFuture,
    queued: Arc<AtomicBool>,
    completed: Arc<AtomicBool>,
    waiting_on: Option<u64>,
}

impl WorkerTask {
    /// Creates a worker task wrapping `future` for connection `conn_id`,
    /// labeled with `trace_task_id` for tracing.
    pub fn new(conn_id: Option<u64>, trace_task_id: TaskID, future: WorkerTaskFuture) -> Self {
        Self {
            conn_id,
            trace_task_id,
            future,
            queued: Arc::new(AtomicBool::new(false)),
            completed: Arc::new(AtomicBool::new(false)),
            waiting_on: None,
        }
    }

    /// Returns the id of the connection this task belongs to, if any.
    pub fn conn_id(&self) -> Option<u64> {
        self.conn_id
    }

    /// Returns the task id used for tracing.
    pub fn trace_task_id(&self) -> TaskID {
        self.trace_task_id
    }

    /// Returns a pinned mutable reference to the inner future so the
    /// registry can poll it.
    pub fn future_mut(&mut self) -> WorkerTaskFutureRef<'_> {
        self.future.as_mut()
    }

    /// Returns the shared flag set while the task sits in the ready queue.
    pub fn queued(&self) -> &Arc<AtomicBool> {
        &self.queued
    }

    /// Returns the shared flag set once the task has run to completion.
    pub fn completed(&self) -> &Arc<AtomicBool> {
        &self.completed
    }

    /// Clears the queued flag; called when the task leaves the ready queue.
    pub fn clear_queued(&self) {
        self.queued.store(false, Ordering::Release);
    }

    /// Takes the id of the IO operation the task is blocked on, leaving
    /// `None` behind.
    pub fn take_waiting_on(&mut self) -> Option<u64> {
        self.waiting_on.take()
    }

    /// Records the id of the IO operation the task is blocked on.
    pub fn set_waiting_on(&mut self, op_id: u64) {
        self.waiting_on = Some(op_id);
    }
}

type WorkerTaskFutureRef<'a> = Pin<&'a mut (dyn Future<Output = RS<()>> + 'static)>;
