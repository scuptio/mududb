use std::cell::UnsafeCell;
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use crate::task::async_::try_this_task_id;
use crate::task::id::TaskID;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;

use crate::imp::native::linux::io_uring::file::{
    FileInflightOp, FileIoRequest, complete_file_io, submit_file_io,
};
use crate::imp::native::linux::io_uring::path::{
    PathInflightOp, PathIoRequest, complete_path_io, submit_path_io,
};
use crate::imp::native::linux::io_uring::socket::{
    SocketInflightOp, SocketIoRequest, complete_socket_io, submit_socket_io,
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
}

impl Default for WorkerLocalRing {
    fn default() -> Self {
        Self::new()
    }
}

impl WorkerLocalRing {
    #[allow(dead_code)]
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
            .map_err(|_| m_error!(EC::InternalErr, "worker local ring lock poisoned"))?
            .insert(op_id, op);
        if let Some(task_id) = try_this_task_id() {
            if let Some(ctx) = crate::task::context::TaskContext::get(task_id) {
                ctx.watch("io.registered_op_id", &op_id.to_string());
                ctx.watch("io.registered_op_kind", op_kind);
            }
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

    pub fn requeue_front(&self, op_id: u64, op: WorkerRingOp) -> RS<()> {
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

    pub fn take_pending(&self) -> RS<Option<(u64, WorkerRingOp)>> {
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

    pub fn task_for_op(&self, op_id: u64) -> Option<TaskID> {
        self.op_tasks.lock().ok()?.get(&op_id).copied()
    }

    pub fn finish_op(&self, op_id: u64) {
        if let Ok(mut guard) = self.op_tasks.lock() {
            guard.remove(&op_id);
        }
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
        let ring = ring
            .as_ref()
            .ok_or_else(|| m_error!(EC::NoSuchElement, "current worker ring is not set"))?;
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
