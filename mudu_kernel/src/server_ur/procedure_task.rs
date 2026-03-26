use crate::server_ur::pending_procedure_invocation::PendingProcedureInvocation;
use crate::server_ur::routing::SessionOpenTransferAction;
use mudu::common::id::OID;
use mudu::common::result::RS;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub struct ProcedureTask {
    task_id: u64,
    conn_id: u64,
    request_id: u64,
    // The invoke future stays owned by the worker, so procedure progress is
    // resumed from the ring loop instead of a separate async runtime.
    future: Pin<Box<dyn Future<Output = RS<Vec<u8>>> + 'static>>,
    // This flag coalesces repeated wakeups while the task is already queued
    // for polling in the worker loop.
    queued: Arc<AtomicBool>,
    completed: Arc<AtomicBool>,
    // A pending poll is represented as a worker-local op id. Once that op is
    // completed, the task is re-enqueued and polled again from the same thread.
    waiting_on: Option<u64>,
}

pub(in crate::server_ur) enum FrameDispatch {
    Immediate(Vec<u8>),
    Pending(PendingProcedureInvocation),
    Transfer(SessionTransferDispatch),
}

pub(in crate::server_ur) struct SessionTransferDispatch {
    target_worker: usize,
    session_ids: Vec<OID>,
    action: SessionOpenTransferAction,
}

impl SessionTransferDispatch {
    pub(in crate::server_ur) fn new(
        target_worker: usize,
        session_ids: Vec<OID>,
        action: SessionOpenTransferAction,
    ) -> Self {
        Self {
            target_worker,
            session_ids,
            action,
        }
    }

    pub(in crate::server_ur) fn target_worker(&self) -> usize {
        self.target_worker
    }

    pub(in crate::server_ur) fn session_ids(&self) -> &[OID] {
        &self.session_ids
    }

    pub(in crate::server_ur) fn action(&self) -> SessionOpenTransferAction {
        self.action
    }
}

impl ProcedureTask {
    pub(in crate::server_ur) fn new(
        task_id: u64,
        conn_id: u64,
        request_id: u64,
        future: Pin<Box<dyn Future<Output = RS<Vec<u8>>> + 'static>>,
        completed: Arc<AtomicBool>,
    ) -> Self {
        Self {
            task_id,
            conn_id,
            request_id,
            future,
            queued: Arc::new(AtomicBool::new(false)),
            completed,
            waiting_on: None,
        }
    }

    pub(in crate::server_ur) fn conn_id(&self) -> u64 {
        self.conn_id
    }

    pub(in crate::server_ur) fn request_id(&self) -> u64 {
        self.request_id
    }

    pub(in crate::server_ur) fn future_mut(
        &mut self,
    ) -> Pin<&mut (dyn Future<Output = RS<Vec<u8>>> + 'static)> {
        self.future.as_mut()
    }

    pub(in crate::server_ur) fn queued(&self) -> &Arc<AtomicBool> {
        &self.queued
    }

    pub(in crate::server_ur) fn completed(&self) -> &Arc<AtomicBool> {
        &self.completed
    }
    pub(in crate::server_ur) fn clear_queued(&self) {
        self.queued.store(false, Ordering::Release);
    }

    pub(in crate::server_ur) fn waiting_on(&self) -> Option<u64> {
        self.waiting_on
    }

    pub(in crate::server_ur) fn take_waiting_on(&mut self) -> Option<u64> {
        self.waiting_on.take()
    }

    pub(in crate::server_ur) fn set_waiting_on(&mut self, op_id: u64) {
        self.waiting_on = Some(op_id);
    }
}
