use std::cell::UnsafeCell;
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::task::Waker;

use crate::task::async_::try_this_task_id;
use crate::task::id::TaskID;
use crate::time::Instant;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;

use crate::imp::native::linux::io_uring::file::{
    complete_file_io, submit_file_io, FileInflightOp, FileIoRequest,
};
use crate::imp::native::linux::io_uring::path::{
    complete_path_io, submit_path_io, PathInflightOp, PathIoRequest,
};
use crate::imp::native::linux::io_uring::socket::{
    complete_socket_io, submit_socket_io, SocketInflightOp, SocketIoRequest,
};
use crate::server::task_registry::WorkerTaskRegistry;

thread_local! {
    static CURRENT_WORKER_RING: UnsafeCell<Option<Arc<WorkerLocalRing>>> =
        const { UnsafeCell::new(None) };
}

pub enum WorkerRingOp {
    File(FileIoRequest),
    Path(PathIoRequest),
    Socket(SocketIoRequest),
}

pub enum UserIoInflight {
    File { op_id: u64, op: FileInflightOp },
    Path { op_id: u64, op: PathInflightOp },
    Socket { op_id: u64, op: SocketInflightOp },
}

impl UserIoInflight {
    pub fn op_id(&self) -> u64 {
        match self {
            Self::File { op_id, .. } => *op_id,
            Self::Path { op_id, .. } => *op_id,
            Self::Socket { op_id, .. } => *op_id,
        }
    }

    pub fn kind(&self) -> &'static str {
        match self {
            Self::File { op, .. } => op.kind(),
            Self::Path { op, .. } => op.kind(),
            Self::Socket { .. } => "socket",
        }
    }
}

pub struct WorkerLocalRing {
    worker_tasks: WorkerTaskRegistry,
    next_op_id: AtomicU64,
    pending: Mutex<VecDeque<u64>>,
    ops: Mutex<HashMap<u64, WorkerRingOp>>,
    op_tasks: Mutex<HashMap<u64, TaskID>>,
    timeouts: Mutex<BTreeMap<(Instant, u64), Waker>>,
    next_timeout_id: AtomicU64,
}

impl Default for WorkerLocalRing {
    fn default() -> Self {
        Self::new()
    }
}

impl WorkerLocalRing {
    pub fn new() -> Self {
        Self::new_with_task_wake_fd(None)
    }

    pub fn new_with_task_wake_fd(wake_fd: Option<i32>) -> Self {
        Self {
            worker_tasks: WorkerTaskRegistry::new_with_wake_fd(wake_fd),
            next_op_id: AtomicU64::new(1),
            pending: Mutex::new(VecDeque::new()),
            ops: Mutex::new(HashMap::new()),
            op_tasks: Mutex::new(HashMap::new()),
            timeouts: Mutex::new(BTreeMap::new()),
            next_timeout_id: AtomicU64::new(1),
        }
    }

    pub fn worker_task_registry(&self) -> &WorkerTaskRegistry {
        &self.worker_tasks
    }

    pub fn register(&self, op: WorkerRingOp) -> RS<u64> {
        let op_id = self.next_op_id.fetch_add(1, Ordering::Relaxed);
        let op_kind = match &op {
            WorkerRingOp::File(request) => request.kind(),
            WorkerRingOp::Path(request) => request.kind(),
            WorkerRingOp::Socket(_) => "socket",
        };
        tracing::debug!(op_id, kind = op_kind, "worker local ring register op");
        self.ops
            .lock()
            .map_err(|_| mudu_error!(ErrorCode::Internal, "worker local ring lock poisoned"))?
            .insert(op_id, op);
        if let Some(task_id) = try_this_task_id() {
            if let Some(ctx) = crate::task::context::TaskContext::get(task_id) {
                ctx.watch("io.registered_op_id", &op_id.to_string());
                ctx.watch("io.registered_op_kind", op_kind);
            }
            self.op_tasks
                .lock()
                .map_err(|_| mudu_error!(ErrorCode::Internal, "worker local ring lock poisoned"))?
                .insert(op_id, task_id);
        }
        self.pending
            .lock()
            .map_err(|_| mudu_error!(ErrorCode::Internal, "worker local ring lock poisoned"))?
            .push_back(op_id);
        Ok(op_id)
    }

    pub fn requeue_front(&self, op_id: u64, op: WorkerRingOp) -> RS<()> {
        self.ops
            .lock()
            .map_err(|_| mudu_error!(ErrorCode::Internal, "worker local ring lock poisoned"))?
            .insert(op_id, op);
        self.pending
            .lock()
            .map_err(|_| mudu_error!(ErrorCode::Internal, "worker local ring lock poisoned"))?
            .push_front(op_id);
        Ok(())
    }

    pub fn take_pending(&self) -> RS<Option<(u64, WorkerRingOp)>> {
        let Some(op_id) = self
            .pending
            .lock()
            .map_err(|_| mudu_error!(ErrorCode::Internal, "worker local ring lock poisoned"))?
            .pop_front()
        else {
            return Ok(None);
        };
        let op = self
            .ops
            .lock()
            .map_err(|_| mudu_error!(ErrorCode::Internal, "worker local ring lock poisoned"))?
            .remove(&op_id)
            .ok_or_else(|| {
                mudu_error!(
                    ErrorCode::Internal,
                    format!("worker local ring op {} missing from registry", op_id)
                )
            })?;
        Ok(Some((op_id, op)))
    }

    pub fn task_for_op(&self, op_id: u64) -> Option<TaskID> {
        self.op_tasks.lock().ok()?.get(&op_id).copied()
    }

    pub fn finish_op(&self, op_id: u64) {
        if let Ok(mut guard) = self.op_tasks.lock() {
            guard.remove(&op_id);
        }
    }

    /// Registers `waker` to be woken once `deadline` has passed.
    ///
    /// The io_uring worker service loop is a synchronous loop that never
    /// yields to the tokio runtime, so tokio timers never advance on worker
    /// threads. This heap is the worker-local clock source used by
    /// `mudu_sys::task::async_::timeout`/`sleep` instead; the service loop
    /// drains it via `take_expired_timeouts` and bounds its CQE wait with
    /// `next_timeout_deadline`.
    ///
    /// Returns a registration id; pass it together with `deadline` to
    /// `remove_timeout` when the registration becomes stale (e.g. the future
    /// is re-polled with a new waker or completes before the deadline).
    pub fn register_timeout(&self, deadline: Instant, waker: Waker) -> RS<u64> {
        let id = self.next_timeout_id.fetch_add(1, Ordering::Relaxed);
        self.timeouts
            .lock()
            .map_err(|_| mudu_error!(ErrorCode::Internal, "worker local ring lock poisoned"))?
            .insert((deadline, id), waker);
        Ok(id)
    }

    pub fn remove_timeout(&self, deadline: Instant, id: u64) -> RS<()> {
        self.timeouts
            .lock()
            .map_err(|_| mudu_error!(ErrorCode::Internal, "worker local ring lock poisoned"))?
            .remove(&(deadline, id));
        Ok(())
    }

    /// Pops all registrations whose deadline is at or before `now`.
    pub fn take_expired_timeouts(&self, now: Instant) -> RS<Vec<Waker>> {
        let mut guard = self
            .timeouts
            .lock()
            .map_err(|_| mudu_error!(ErrorCode::Internal, "worker local ring lock poisoned"))?;
        let mut expired = Vec::new();
        while let Some((&(deadline, _), _)) = guard.iter().next() {
            if deadline > now {
                break;
            }
            if let Some((_, waker)) = guard.pop_first() {
                expired.push(waker);
            }
        }
        Ok(expired)
    }

    pub fn next_timeout_deadline(&self) -> RS<Option<Instant>> {
        let guard = self
            .timeouts
            .lock()
            .map_err(|_| mudu_error!(ErrorCode::Internal, "worker local ring lock poisoned"))?;
        Ok(guard.keys().next().map(|(deadline, _)| *deadline))
    }
}

pub fn set_current_worker_ring(ring: Arc<WorkerLocalRing>) {
    CURRENT_WORKER_RING.with(|slot| {
        // Safety: this slot is thread-local and only accessed through these helpers.
        unsafe {
            *slot.get() = Some(ring);
        }
    });
}

pub fn unset_current_worker_ring() {
    CURRENT_WORKER_RING.with(|slot| {
        // Safety: this slot is thread-local and only accessed through these helpers.
        unsafe {
            *slot.get() = None;
        }
    });
}

pub fn has_current_worker_ring() -> bool {
    CURRENT_WORKER_RING.with(|slot| {
        // Safety: shared reads are confined to the current thread-local slot.
        unsafe { (*slot.get()).is_some() }
    })
}

pub fn with_current_ring<F, R>(f: F) -> RS<R>
where
    F: FnOnce(&Arc<WorkerLocalRing>) -> RS<R>,
{
    CURRENT_WORKER_RING.with(|slot| {
        // Safety: shared reads are confined to the current thread-local slot.
        let ring = unsafe { &*slot.get() };
        let ring = ring.as_ref().ok_or_else(|| {
            mudu_error!(ErrorCode::EntityNotFound, "current worker ring is not set")
        })?;
        f(ring)
    })
}

pub fn submit_user_ring_op(
    op_id: u64,
    op: WorkerRingOp,
    sqe: &mut crate::imp::native::linux::io_uring::iouring::SubmissionQueueEntry<'_>,
) -> UserIoInflight {
    match op {
        WorkerRingOp::File(request) => UserIoInflight::File {
            op_id,
            op: submit_file_io(request, sqe),
        },
        WorkerRingOp::Path(request) => UserIoInflight::Path {
            op_id,
            op: submit_path_io(request, sqe),
        },
        WorkerRingOp::Socket(request) => UserIoInflight::Socket {
            op_id,
            op: submit_socket_io(request, sqe),
        },
    }
}

pub fn complete_user_ring_op(op: UserIoInflight, result: i32, ring: &WorkerLocalRing) -> RS<()> {
    let (op_id, done) = match op {
        UserIoInflight::File { op_id, op } => (op_id, complete_file_io(op_id, op, result, ring)?),
        UserIoInflight::Path { op_id, op } => (op_id, complete_path_io(op_id, op, result, ring)?),
        UserIoInflight::Socket { op_id, op } => {
            (op_id, complete_socket_io(op_id, op, result, ring)?)
        }
    };
    if done {
        ring.finish_op(op_id);
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use futures::task::noop_waker;
    use std::time::Duration;

    #[test]
    fn timeout_heap_orders_expires_and_removes() {
        let ring = WorkerLocalRing::new();
        let now = Instant::now();
        let late_deadline = now + Duration::from_millis(50);
        let early_deadline = now + Duration::from_millis(10);

        let late_id = ring.register_timeout(late_deadline, noop_waker()).unwrap();
        ring.register_timeout(early_deadline, noop_waker()).unwrap();

        // The nearest deadline is reported first.
        assert_eq!(ring.next_timeout_deadline().unwrap(), Some(early_deadline));
        // Nothing is expired at `now`.
        assert!(ring.take_expired_timeouts(now).unwrap().is_empty());
        // Once the earlier deadline passes, exactly one entry fires.
        let expired = ring.take_expired_timeouts(early_deadline).unwrap();
        assert_eq!(expired.len(), 1);
        assert_eq!(ring.next_timeout_deadline().unwrap(), Some(late_deadline));

        // Removing a stale registration empties the heap.
        ring.remove_timeout(late_deadline, late_id).unwrap();
        assert_eq!(ring.next_timeout_deadline().unwrap(), None);
        // Removing an unknown registration is a no-op.
        ring.remove_timeout(late_deadline, late_id).unwrap();
    }
}
