#![allow(dead_code)]

use mudu::common::result::RS;
use mudu_utils::task_id::TaskID;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub struct AsyncFuncTask {
    conn_id: u64,
    trace_task_id: TaskID,
    request_id: u64,
    future: AsyncFuncFuture,
    queued: Arc<AtomicBool>,
    completed: Arc<AtomicBool>,
    waiting_on: Option<u64>,
}

pub(in crate::server) enum HandleResult {
    Response(Vec<u8>),
}

impl AsyncFuncTask {
    pub(in crate::server) fn new(
        conn_id: u64,
        trace_task_id: TaskID,
        request_id: u64,
        future: AsyncFuncFuture,
        completed: Arc<AtomicBool>,
    ) -> Self {
        Self {
            conn_id,
            trace_task_id,
            request_id,
            future,
            queued: Arc::new(AtomicBool::new(false)),
            completed,
            waiting_on: None,
        }
    }

    pub(in crate::server) fn conn_id(&self) -> u64 {
        self.conn_id
    }

    pub(in crate::server) fn trace_task_id(&self) -> TaskID {
        self.trace_task_id
    }

    pub(in crate::server) fn request_id(&self) -> u64 {
        self.request_id
    }

    pub(in crate::server) fn future_mut(&mut self) -> AsyncFuncFutureRef<'_> {
        self.future.as_mut()
    }

    pub(in crate::server) fn queued(&self) -> &Arc<AtomicBool> {
        &self.queued
    }

    pub(in crate::server) fn completed(&self) -> &Arc<AtomicBool> {
        &self.completed
    }

    pub(in crate::server) fn clear_queued(&self) {
        self.queued.store(false, Ordering::Release);
    }

    pub(in crate::server) fn take_waiting_on(&mut self) -> Option<u64> {
        self.waiting_on.take()
    }

    pub(in crate::server) fn set_waiting_on(&mut self, op_id: u64) {
        self.waiting_on = Some(op_id);
    }
}

pub(in crate::server) type AsyncFuncFuture =
    Pin<Box<dyn Future<Output = RS<HandleResult>> + 'static>>;

type AsyncFuncFutureRef<'a> = Pin<&'a mut (dyn Future<Output = RS<HandleResult>> + 'static)>;
