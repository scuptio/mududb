use std::cell::UnsafeCell;
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use mudu_utils::task_async::try_this_task_id;
use mudu_utils::task_id::TaskID;

use crate::io::file::{complete_file_io, submit_file_io, FileInflightOp, FileIoRequest};
use crate::io::path::{complete_path_io, submit_path_io, PathInflightOp, PathIoRequest};
use crate::io::socket::{complete_socket_io, submit_socket_io, SocketInflightOp, SocketIoRequest};
use crate::server::task_registry::WorkerTaskRegistry;

thread_local! {
    static CURRENT_WORKER_RING: UnsafeCell<Option<Arc<WorkerLocalRing>>> =
        const { UnsafeCell::new(None) };
}

pub(crate) enum WorkerRingOp {
    File(FileIoRequest),
    Path(PathIoRequest),
    Socket(SocketIoRequest),
}

pub(crate) enum UserIoInflight {
    File { op_id: u64, op: FileInflightOp },
    Path { op_id: u64, op: PathInflightOp },
    Socket { op_id: u64, op: SocketInflightOp },
}

impl UserIoInflight {
    pub(crate) fn op_id(&self) -> u64 {
        match self {
            Self::File { op_id, .. } => *op_id,
            Self::Path { op_id, .. } => *op_id,
            Self::Socket { op_id, .. } => *op_id,
        }
    }

    pub(crate) fn kind(&self) -> &'static str {
        match self {
            Self::File { .. } => "file",
            Self::Path { .. } => "path",
            Self::Socket { .. } => "socket",
        }
    }
}

pub(crate) struct WorkerLocalRing {
    worker_tasks: WorkerTaskRegistry,
    next_op_id: AtomicU64,
    pending: Mutex<VecDeque<u64>>,
    ops: Mutex<HashMap<u64, WorkerRingOp>>,
    op_tasks: Mutex<HashMap<u64, TaskID>>,
}

impl WorkerLocalRing {
    pub(crate) fn new() -> Self {
        Self {
            worker_tasks: WorkerTaskRegistry::new(),
            next_op_id: AtomicU64::new(1),
            pending: Mutex::new(VecDeque::new()),
            ops: Mutex::new(HashMap::new()),
            op_tasks: Mutex::new(HashMap::new()),
        }
    }

    pub fn worker_task_registry(&self) -> &WorkerTaskRegistry {
        &self.worker_tasks
    }

    pub(crate) fn register(&self, op: WorkerRingOp) -> RS<u64> {
        let op_id = self.next_op_id.fetch_add(1, Ordering::Relaxed);
        self.ops
            .lock()
            .map_err(|_| m_error!(EC::InternalErr, "worker local ring lock poisoned"))?
            .insert(op_id, op);
        if let Some(task_id) = try_this_task_id() {
            self.op_tasks
                .lock()
                .map_err(|_| m_error!(EC::InternalErr, "worker local ring lock poisoned"))?
                .insert(op_id, task_id);
        }
        self.pending
            .lock()
            .map_err(|_| m_error!(EC::InternalErr, "worker local ring lock poisoned"))?
            .push_back(op_id);
        Ok(op_id)
    }

    pub(crate) fn requeue_front(&self, op_id: u64, op: WorkerRingOp) -> RS<()> {
        self.ops
            .lock()
            .map_err(|_| m_error!(EC::InternalErr, "worker local ring lock poisoned"))?
            .insert(op_id, op);
        self.pending
            .lock()
            .map_err(|_| m_error!(EC::InternalErr, "worker local ring lock poisoned"))?
            .push_front(op_id);
        Ok(())
    }

    pub(crate) fn take_pending(&self) -> RS<Option<(u64, WorkerRingOp)>> {
        let Some(op_id) = self
            .pending
            .lock()
            .map_err(|_| m_error!(EC::InternalErr, "worker local ring lock poisoned"))?
            .pop_front()
        else {
            return Ok(None);
        };
        let op = self
            .ops
            .lock()
            .map_err(|_| m_error!(EC::InternalErr, "worker local ring lock poisoned"))?
            .remove(&op_id)
            .ok_or_else(|| {
                m_error!(
                    EC::InternalErr,
                    format!("worker local ring op {} missing from registry", op_id)
                )
            })?;
        Ok(Some((op_id, op)))
    }

    pub(crate) fn task_for_op(&self, op_id: u64) -> Option<TaskID> {
        self.op_tasks.lock().ok()?.get(&op_id).copied()
    }

    pub(crate) fn finish_op(&self, op_id: u64) {
        if let Ok(mut guard) = self.op_tasks.lock() {
            guard.remove(&op_id);
        }
    }
}

pub(crate) fn set_current_worker_ring(ring: Arc<WorkerLocalRing>) {
    CURRENT_WORKER_RING.with(|slot| {
        // Safety: this slot is thread-local and only accessed through these helpers.
        unsafe {
            *slot.get() = Some(ring);
        }
    });
}

pub(crate) fn unset_current_worker_ring() {
    CURRENT_WORKER_RING.with(|slot| {
        // Safety: this slot is thread-local and only accessed through these helpers.
        unsafe {
            *slot.get() = None;
        }
    });
}

pub(crate) fn has_current_worker_ring() -> bool {
    CURRENT_WORKER_RING.with(|slot| {
        // Safety: shared reads are confined to the current thread-local slot.
        unsafe { (*slot.get()).is_some() }
    })
}

pub(crate) fn with_current_ring<F, R>(f: F) -> RS<R>
where
    F: FnOnce(&Arc<WorkerLocalRing>) -> RS<R>,
{
    CURRENT_WORKER_RING.with(|slot| {
        // Safety: shared reads are confined to the current thread-local slot.
        let ring = unsafe { &*slot.get() };
        let ring = ring
            .as_ref()
            .ok_or_else(|| m_error!(EC::NoSuchElement, "current worker ring is not set"))?;
        f(ring)
    })
}

pub(crate) fn submit_user_ring_op(
    op_id: u64,
    op: WorkerRingOp,
    sqe: &mut mudu_sys::uring::SubmissionQueueEntry<'_>,
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

pub(crate) fn complete_user_ring_op(
    op: UserIoInflight,
    result: i32,
    ring: &WorkerLocalRing,
) -> RS<()> {
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
