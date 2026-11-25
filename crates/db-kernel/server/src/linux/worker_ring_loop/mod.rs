#[cfg(test)]
use crate::callback_registry::{
    AsyncCallback, CallbackDomain, CallbackEventKey, CallbackId, CallbackRegistry, CallbackTrigger,
    PendingCallback,
};
use crate::connection_worker_task::spawn_connection_worker_task;
use crate::fs_gc::FS_GC_INTERVAL;
use crate::inflight_op::{AcceptOp, InflightOp};
use crate::loop_mailbox::{
    drain_messages, handle_read_completion, submit_read_if_needed, LoopMailboxSubmitCtx,
};
use crate::loop_user_io::{
    handle_completion as handle_user_io_completion, submit as submit_user_io, LoopUserIoCtx,
};
use crate::task;
use crate::worker::flush_worker_dirty_pages;
use crate::worker::WorkerRuntime;
use crate::worker::{as_worker_local_ref, new_session_bound_worker_runtime};
use crate::worker_channel_uring::UringWorkerChannel;
use crate::worker_loop_stats::WorkerLoopStats;
use crate::worker_mailbox::WorkerMailboxMsg;
use ::conn::worker_local::{set_current_worker_local, unset_current_worker_local};
use ::xcontract::worker_storage::DIRTY_PAGE_FLUSH_INTERVAL;
use mudu_sys::io::worker_ring::{
    set_current_worker_ring, unset_current_worker_ring, WorkerLocalRing,
};
use mudu_utils::channel::{
    set_current_channel, spawn_endpoint, unset_current_channel, EndpointGuard,
};

use ::wal::worker_log::ChunkedWorkerLogBackend;
use ::wal::xl_batch_worker_log::{new_xl_batch_worker_log, XLBatchWorkerLog};
use crossbeam_queue::SegQueue;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_utils::notifier::Notifier;
use mudu_utils::scoped_task_trace;
use mudu_utils::task_context::TaskContext;
use std::collections::HashMap;

use std::future::Future;
use std::os::fd::RawFd;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use mudu_sys::sync::{SCondvar, SMutex};
use std::time::Duration;

#[cfg(all(test, not(feature = "ds")))]
use mudu_sys::net::sync::StdTcpListener;
use tracing::{debug, error, trace};

pub(crate) fn sockaddr_to_socket_addr(
    storage: &mudu_sys::io::iouring::SockAddrBuf,
) -> RS<std::net::SocketAddr> {
    mudu_sys::io::net::sockaddr_to_socket_addr(storage)
}

pub(crate) struct RecoveryCoordinator {
    total_workers: usize,
    ready_notifier: Option<Notifier>,
    state: SMutex<RecoveryState>,
    condvar: SCondvar,
}
#[derive(Default)]
struct RecoveryState {
    recovered_workers: usize,
    failed: bool,
}

impl RecoveryCoordinator {
    pub fn new(total_workers: usize, ready_notifier: Option<Notifier>) -> Self {
        Self {
            total_workers,
            ready_notifier,
            state: SMutex::new(RecoveryState::default()),
            condvar: SCondvar::new(),
        }
    }

    pub(crate) fn worker_succeeded(&self) -> RS<()> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| mudu_error!(ErrorCode::Internal, "recovery coordinator lock poisoned"))?;
        if state.failed {
            return Err(mudu_error!(
                ErrorCode::Thread,
                "worker recovery aborted because another worker failed"
            ));
        }
        state.recovered_workers += 1;
        trace!(
            recovered_workers = state.recovered_workers,
            total_workers = self.total_workers,
            "iouring recovery coordinator worker reached barrier"
        );
        if state.recovered_workers == self.total_workers {
            // In io_uring mode the listener can start accepting sockets before
            // every worker has finished WAL recovery. Publish readiness only
            // after the final worker reaches the common recovery barrier so
            // callers do not race listener availability with service
            // availability.
            if let Some(ready_notifier) = &self.ready_notifier {
                trace!("iouring recovery coordinator publishing ready barrier");
                ready_notifier.notify_all();
            }
            self.condvar.notify_all();
            return Ok(());
        }
        // Recovery must be complete on every worker before the service loop
        // starts. If one worker fails recovery, wake everybody and abort
        // instead of leaving the successful workers stuck forever.
        while !state.failed && state.recovered_workers < self.total_workers {
            trace!(
                recovered_workers = state.recovered_workers,
                total_workers = self.total_workers,
                "iouring recovery coordinator waiting for peers"
            );
            state = self.condvar.wait(state).map_err(|err| {
                mudu_error!(
                    ErrorCode::Internal,
                    "recovery coordinator condvar wait poisoned",
                    err
                )
            })?;
        }
        if state.failed {
            return Err(mudu_error!(
                ErrorCode::Thread,
                "worker recovery aborted because another worker failed"
            ));
        }
        Ok(())
    }

    pub(crate) fn worker_failed(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.failed = true;
            self.condvar.notify_all();
        }
    }
}

mod bootstrap;
mod bootstrap_waker;
#[cfg(test)]
mod callbacks;
mod completion;
mod recovery;
mod runtime;
mod submit;
#[cfg(test)]
mod tests;

pub(crate) use bootstrap::drive_future_with_ring;

type XLWorkerLog =
    XLBatchWorkerLog<ChunkedWorkerLogBackend, recovery::WorkerRingLoopRecoveryHandler>;
/// Drives a single io_uring worker event loop.
///
/// The loop owns the worker-local ring and multiplexes several kinds of work:
/// accepting new sockets, consuming inter-worker mailbox notifications,
/// completing user-triggered file/socket I/O, and coordinating connection task
/// lifecycle. It also performs worker-log recovery before the steady-state loop
/// starts so replayed state is visible to newly accepted connections.
pub(crate) struct WorkerRingLoop {
    worker: WorkerRuntime,
    log: Option<XLWorkerLog>,
    ring: mudu_sys::io::iouring::IoUring,
    listener_fd: RawFd,
    mailbox_fd: RawFd,
    mailbox: Arc<SegQueue<WorkerMailboxMsg>>,
    conn_id_alloc: Arc<AtomicU64>,
    recovery_coordinator: Arc<RecoveryCoordinator>,
    worker_local_ring: Arc<WorkerLocalRing>,
    worker_channel: Arc<UringWorkerChannel>,
    /// Owns the channel endpoint registration installed by `initialize`;
    /// dropped by `teardown_worker_registration`.
    endpoint_guard: Option<EndpointGuard>,
    connection_task_fds: Arc<SMutex<HashMap<u64, RawFd>>>,
    #[cfg(test)]
    callback_registry: CallbackRegistry,
    #[cfg(test)]
    callback_sequence_frontiers: HashMap<CallbackDomain, u64>,
    inflight: HashMap<u64, InflightOp>,
    next_token: u64,
    mailbox_read_submitted: bool,
    shutdown_triggered: AtomicBool,
    shutting_down: bool,
    checkpoint_started: bool,
    checkpoint_failed: Arc<AtomicBool>,
    accept_submitted: bool,
    stop: Arc<AtomicBool>,
    stats: WorkerLoopStats,
    fs_gc_next_due: mudu_sys::time::Instant,
    fs_gc_inflight: Arc<AtomicBool>,
    page_flush_next_due: mudu_sys::time::Instant,
    page_flush_inflight: Arc<AtomicBool>,
}

#[cfg(test)]
pub(crate) struct WorkerRingLoopArgs {
    pub worker: WorkerRuntime,
    pub listener_fd: RawFd,
    pub mailbox_fd: RawFd,
    pub mailbox: Arc<SegQueue<WorkerMailboxMsg>>,
    pub mailboxes: Vec<Arc<SegQueue<WorkerMailboxMsg>>>,
    pub mailbox_fds: Vec<RawFd>,
    pub conn_id_alloc: Arc<AtomicU64>,
    pub recovery_coordinator: Arc<RecoveryCoordinator>,
    pub stop: Arc<AtomicBool>,
}

pub(crate) struct WorkerRingLoopWithRingArgs {
    pub worker: WorkerRuntime,
    pub listener_fd: RawFd,
    pub mailbox_fd: RawFd,
    pub mailbox: Arc<SegQueue<WorkerMailboxMsg>>,
    pub mailboxes: Vec<Arc<SegQueue<WorkerMailboxMsg>>>,
    pub mailbox_fds: Vec<RawFd>,
    pub conn_id_alloc: Arc<AtomicU64>,
    pub recovery_coordinator: Arc<RecoveryCoordinator>,
    pub stop: Arc<AtomicBool>,
    pub ring: mudu_sys::io::iouring::IoUring,
    pub worker_local_ring: Arc<WorkerLocalRing>,
}

impl WorkerRingLoop {
    /// Builds the runtime state for one worker loop and initializes its
    /// private io_uring instance.
    #[cfg(test)]
    pub fn new(args: WorkerRingLoopArgs) -> RS<Self> {
        let WorkerRingLoopArgs {
            worker,
            listener_fd,
            mailbox_fd,
            mailbox,
            mailboxes,
            mailbox_fds,
            conn_id_alloc,
            recovery_coordinator,
            stop,
        } = args;
        let ring = Self::new_ring_for_worker(0)?;
        #[allow(clippy::arc_with_non_send_sync)]
        let worker_local_ring = Arc::new(WorkerLocalRing::new_with_task_wake_fd(Some(mailbox_fd)));
        Self::new_with_ring(WorkerRingLoopWithRingArgs {
            worker,
            listener_fd,
            mailbox_fd,
            mailbox,
            mailboxes,
            mailbox_fds,
            conn_id_alloc,
            recovery_coordinator,
            stop,
            ring,
            worker_local_ring,
        })
    }

    pub(crate) fn new_with_ring(args: WorkerRingLoopWithRingArgs) -> RS<Self> {
        let WorkerRingLoopWithRingArgs {
            worker,
            listener_fd,
            mailbox_fd,
            mailbox,
            mailboxes,
            mailbox_fds,
            conn_id_alloc,
            recovery_coordinator,
            stop,
            ring,
            worker_local_ring,
        } = args;
        let worker_id = worker.worker_index();
        let log = worker.worker_log()?.map(|backend| {
            new_xl_batch_worker_log(
                backend.clone(),
                recovery::WorkerRingLoopRecoveryHandler {
                    worker: worker.clone(),
                },
            )
        });
        let worker_channel = UringWorkerChannel::new(
            worker.worker_id(),
            worker.server_instance_id(),
            worker.registry().clone(),
            mailbox_fds.clone(),
            mailboxes.clone(),
        );
        Ok(Self {
            log,
            worker,
            ring,
            listener_fd,
            mailbox_fd,
            mailbox,
            conn_id_alloc,
            recovery_coordinator,
            worker_local_ring,
            worker_channel,
            endpoint_guard: None,
            connection_task_fds: Arc::new(SMutex::new(HashMap::new())),
            #[cfg(test)]
            callback_registry: CallbackRegistry::new(),
            #[cfg(test)]
            callback_sequence_frontiers: HashMap::new(),
            inflight: HashMap::new(),
            next_token: 1,
            mailbox_read_submitted: false,
            shutdown_triggered: AtomicBool::new(false),
            shutting_down: false,
            checkpoint_started: false,
            checkpoint_failed: Arc::new(AtomicBool::new(false)),
            accept_submitted: false,
            stop,
            stats: WorkerLoopStats {
                worker_id,
                ..WorkerLoopStats::default()
            },
            fs_gc_next_due: mudu_sys::time::instant_now() + FS_GC_INTERVAL,
            fs_gc_inflight: Arc::new(AtomicBool::new(false)),
            page_flush_next_due: mudu_sys::time::instant_now() + DIRTY_PAGE_FLUSH_INTERVAL,
            page_flush_inflight: Arc::new(AtomicBool::new(false)),
        })
    }

    /// EXPERIMENT switch (`MUDU_IOWQ_AFF=1`): pin each worker ring's io_wq
    /// threads to the worker's own core and raise their caps, attacking the
    /// io_wq scheduling latency measured behind `wal_write_wait`.
    pub(crate) fn iowq_affinity_experiment() -> bool {
        static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        *ENABLED.get_or_init(|| {
            mudu_sys::env_var::var("MUDU_IOWQ_AFF")
                .is_some_and(|value| value == "1" || value.eq_ignore_ascii_case("true"))
        })
    }

    /// Creates the worker ring and, when `MUDU_IOWQ_AFF=1`, pins the ring's
    /// io_wq workers to `worker_index`'s own core and raises their caps, so
    /// buffered-write SQEs execute on the same core the worker runs on.
    pub(crate) fn new_ring_for_worker(worker_index: usize) -> RS<mudu_sys::io::iouring::IoUring> {
        let ring = mudu_sys::io::iouring::IoUring::new(1024);
        match ring {
            Ok(mut ring) => {
                if Self::iowq_affinity_experiment() {
                    let rc_aff = ring.register_iowq_affinity(worker_index);
                    let rc_max = ring.register_iowq_max_workers(128, 512);
                    if rc_aff != 0 || rc_max != 0 {
                        tracing::warn!(
                            worker_index,
                            rc_aff,
                            rc_max,
                            "io_uring iowq affinity/max_workers registration failed"
                        );
                    }
                }
                Ok(ring)
            }
            Err(rc) => Err(mudu_error!(
                ErrorCode::Network,
                format!("io_uring_queue_init_params error {}", rc)
            )),
        }
    }

    pub(crate) fn spawn(
        &self,
        conn_id: Option<u64>,
        future: impl Future<Output = RS<()>> + 'static,
    ) {
        self.worker_local_ring
            .worker_task_registry()
            .spawn(conn_id, Box::pin(future));
    }
}
