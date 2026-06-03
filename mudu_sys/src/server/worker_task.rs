use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use mudu::common::result::RS;
use crate::task_id::TaskID;

pub type WorkerTaskFuture = Pin<Box<dyn Future<Output = RS<()>> + 'static>>;

pub struct WorkerTask {
    conn_id: Option<u64>,
    trace_task_id: TaskID,
    future: WorkerTaskFuture,
    queued: Arc<AtomicBool>,
    completed: Arc<AtomicBool>,
    waiting_on: Option<u64>,
}

impl WorkerTask {
    pub fn new(
        conn_id: Option<u64>,
        trace_task_id: TaskID,
        future: WorkerTaskFuture,
    ) -> Self {
        Self {
            conn_id,
            trace_task_id,
            future,
            queued: Arc::new(AtomicBool::new(false)),
            completed: Arc::new(AtomicBool::new(false)),
            waiting_on: None,
        }
    }

    pub fn conn_id(&self) -> Option<u64> {
        self.conn_id
    }

    pub fn trace_task_id(&self) -> TaskID {
        self.trace_task_id
    }

    pub fn future_mut(&mut self) -> WorkerTaskFutureRef<'_> {
        self.future.as_mut()
    }

    pub fn queued(&self) -> &Arc<AtomicBool> {
        &self.queued
    }

    pub fn completed(&self) -> &Arc<AtomicBool> {
        &self.completed
    }

    pub fn clear_queued(&self) {
        self.queued.store(false, Ordering::Release);
    }

    pub fn take_waiting_on(&mut self) -> Option<u64> {
        self.waiting_on.take()
    }

    pub fn set_waiting_on(&mut self, op_id: u64) {
        self.waiting_on = Some(op_id);
    }
}

type WorkerTaskFutureRef<'a> = Pin<&'a mut (dyn Future<Output = RS<()>> + 'static)>;

#[allow(dead_code)]
pub fn spawn_system_worker_task<F>(future: F) -> WorkerTaskFuture
where
    F: Future<Output = RS<()>> + 'static,
{
    Box::pin(async move { future.await })
}
