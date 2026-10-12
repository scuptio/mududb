use crate::async_func_runtime::AsyncFuncInvokerPtr;
use crate::server_cfg::ReplRole;
use crate::server_launch::ServerLaunch;
use crate::worker::{WorkerRuntime, WorkerRuntimeParams};
use crate::worker_loop_stats::WorkerLoopStats;
use crate::worker_mailbox::WorkerMailboxMsg;
use crate::worker_registry::{WorkerIdentity, WorkerRegistry};
use crate::worker_ring_loop::{drive_future_with_ring, WorkerRingLoop, WorkerRingLoopWithRingArgs};
use ::catalog::contract::message_bus_api::ServerInstanceId;
use ::wal::worker_log::{WalSyncPolicy, WorkerLogBatching};
use crossbeam_queue::SegQueue;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_sys::contract::async_io_provider::AsyncIoProvider;
use mudu_sys::io::worker_ring::{set_current_worker_ring, WorkerLocalRing};
use mudu_sys::net::sync::StdTcpListener;
use mudu_sys::sync::{SCondvar, SMutex};
use mudu_sys::task::sync::SJoinHandle;
use mudu_utils::notifier::{Notifier, Waiter};
use mudu_utils::task_async::{build_current_thread_runtime, CurrentThreadTaskRuntime};
use std::os::fd::RawFd;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use mudu_sys::SysIoContext;
use tracing::{debug, trace};

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

pub(crate) fn sync_serve_iouring(
    mut cfg: ServerLaunch,
    stop: Waiter,
    ready: Option<Notifier>,
) -> RS<()> {
    validate_iouring_launch(&cfg)?;
    let shared = IouringSharedCtx::new(&cfg, ready)?;
    let notifier = spawn_shutdown_notifier(
        stop.clone(),
        shared.mailboxes.clone(),
        shared.mailbox_fds.clone(),
        shared.stop_flag.clone(),
    )?;
    let mut handles = Vec::with_capacity(cfg.cfg().worker_count());
    for worker_id in 0..cfg.cfg().worker_count() {
        let args = build_iouring_worker_args(&mut cfg, worker_id, &shared)?;
        handles.push(mudu_sys::task::sync::spawn_thread_named(
            format!("worker-{worker_id}"),
            move || run_iouring_worker(args),
        )?);
    }
    let (worker_stats, first_error) = join_iouring_workers(handles)?;

    if first_error.is_none() {
        let notify_result: RS<()> = notifier
            .join()
            .map_err(|_| mudu_error!(ErrorCode::Thread, "join io_uring shutdown notifier error"))?;
        notify_result?;
        log_worker_stats(&worker_stats);
    }
    for fd in shared.mailbox_fds {
        unsafe {
            libc::close(fd);
        }
    }

    if let Some(err) = first_error {
        return Err(mudu_error!(
            ErrorCode::Thread,
            "io_uring backend stopped due to worker error",
            err
        ));
    }
    Ok(())
}

/// Per-node io_uring state shared by every worker thread.
struct IouringSharedCtx {
    sys: Arc<SysIoContext>,
    conn_id_alloc: Arc<AtomicU64>,
    mailboxes: Vec<Arc<SegQueue<WorkerMailboxMsg>>>,
    mailbox_fds: Vec<RawFd>,
    stop_flag: Arc<AtomicBool>,
    recovery_coordinator: Arc<RecoveryCoordinator>,
}

impl IouringSharedCtx {
    fn new(cfg: &ServerLaunch, ready: Option<Notifier>) -> RS<Self> {
        let sys = SysIoContext::iouring();
        let conn_id_alloc = Arc::new(AtomicU64::new(1));
        let mailboxes: Vec<_> = (0..cfg.cfg().worker_count())
            .map(|_| Arc::new(SegQueue::<WorkerMailboxMsg>::new()))
            .collect();
        let mailbox_fds: Vec<_> = (0..cfg.cfg().worker_count())
            .map(|_| create_mailbox_event_fd())
            .collect::<RS<Vec<_>>>()?;
        let stop_flag = Arc::new(AtomicBool::new(false));
        let recovery_coordinator =
            Arc::new(RecoveryCoordinator::new(cfg.cfg().worker_count(), ready));
        Ok(Self {
            sys,
            conn_id_alloc,
            mailboxes,
            mailbox_fds,
            stop_flag,
            recovery_coordinator,
        })
    }
}

/// Everything one io_uring worker thread owns at spawn time; built on the
/// serve thread and moved into the worker thread.
struct IouringWorkerArgs {
    worker_id: usize,
    listen_addr: std::net::SocketAddr,
    listener: Option<StdTcpListener>,
    sys: Arc<SysIoContext>,
    mailbox_fd: RawFd,
    mailbox: Arc<SegQueue<WorkerMailboxMsg>>,
    all_mailboxes: Vec<Arc<SegQueue<WorkerMailboxMsg>>>,
    all_mailbox_fds: Vec<RawFd>,
    conn_id_alloc: Arc<AtomicU64>,
    worker_identity: WorkerIdentity,
    worker_count: usize,
    data_dir: String,
    log_dir: String,
    log_chunk_size: u64,
    log_batching: WorkerLogBatching,
    wal_sync_policy: WalSyncPolicy,
    procedure_runtime: Option<AsyncFuncInvokerPtr>,
    worker_registry: Arc<WorkerRegistry>,
    server_instance_id: ServerInstanceId,
    async_runtime: Option<Arc<dyn AsyncIoProvider>>,
    stop: Arc<AtomicBool>,
    recovery_coordinator: Arc<RecoveryCoordinator>,
}

fn validate_iouring_launch(cfg: &ServerLaunch) -> RS<()> {
    if cfg.cfg().worker_count() == 0 {
        return Err(mudu_error!(
            ErrorCode::Parse,
            "invalid io_uring worker count"
        ));
    }
    if cfg.cfg().replication().is_some() {
        return Err(mudu_error!(
            ErrorCode::NotImplemented,
            "primary-replica replication is not supported on the io_uring backend; use server mode \"Tokio\""
        ));
    }
    Ok(())
}

/// Spawns the shutdown-notifier thread: when the stop waiter fires it sets
/// the shared stop flag and pushes `Shutdown` into every worker mailbox.
fn spawn_shutdown_notifier(
    stop: Waiter,
    mailboxes: Vec<Arc<SegQueue<WorkerMailboxMsg>>>,
    mailbox_fds: Vec<RawFd>,
    stop_flag: Arc<AtomicBool>,
) -> RS<SJoinHandle<RS<()>>> {
    mudu_sys::task::sync::spawn_thread_named("iouring-shutdown-notifier", move || {
        let runtime = build_current_thread_runtime().map_err(|e| {
            mudu_error!(
                ErrorCode::Tokio,
                "create runtime for io_uring shutdown notifier error",
                e
            )
        })?;
        debug!("iouring shutdown notifier waiting for stop");
        runtime.block_on(stop.wait());
        debug!("iouring shutdown notifier observed stop");
        stop_flag.store(true, Ordering::Relaxed);
        for (mailbox, fd) in mailboxes.into_iter().zip(mailbox_fds) {
            mailbox.push(WorkerMailboxMsg::Shutdown);
            notify_mailbox_fd(fd)?;
        }
        Ok(())
    })
}

fn build_iouring_worker_args(
    cfg: &mut ServerLaunch,
    worker_id: usize,
    shared: &IouringSharedCtx,
) -> RS<IouringWorkerArgs> {
    let worker_port = cfg.cfg().listen_port_for_worker(worker_id)?;
    let listen_addr: std::net::SocketAddr = format!("{}:{}", cfg.cfg().listen_ip(), worker_port)
        .parse()
        .map_err(|e| {
            mudu_error!(
                ErrorCode::Parse,
                "parse io_uring tcp listen address error",
                e
            )
        })?;
    let worker_identity = cfg
        .deps()
        .worker_registry()
        .worker(worker_id)
        .cloned()
        .ok_or_else(|| {
            mudu_error!(
                ErrorCode::EntityNotFound,
                format!("missing worker identity {}", worker_id)
            )
        })?;
    Ok(IouringWorkerArgs {
        worker_id,
        listen_addr,
        listener: cfg.take_prebound_listener(worker_id),
        sys: shared.sys.clone(),
        mailbox_fd: shared.mailbox_fds[worker_id],
        mailbox: shared.mailboxes[worker_id].clone(),
        all_mailboxes: shared.mailboxes.clone(),
        all_mailbox_fds: shared.mailbox_fds.clone(),
        conn_id_alloc: shared.conn_id_alloc.clone(),
        worker_identity,
        worker_count: cfg.cfg().worker_count(),
        data_dir: cfg.cfg().data_dir().to_string(),
        log_dir: cfg.cfg().log_dir().to_string(),
        log_chunk_size: cfg.cfg().log_chunk_size(),
        log_batching: cfg.deps().log_batching(),
        wal_sync_policy: cfg.deps().wal_sync_policy(),
        procedure_runtime: cfg.deps().procedure_runtime_for_worker(worker_id),
        worker_registry: cfg.deps().worker_registry(),
        server_instance_id: cfg.cfg().server_instance_id(),
        async_runtime: Some(shared.sys.provider_arc()),
        stop: shared.stop_flag.clone(),
        recovery_coordinator: shared.recovery_coordinator.clone(),
    })
}

/// Worker thread entry: runs the async worker body and reports a failure to
/// the recovery coordinator so peers blocked on the barrier wake up.
fn run_iouring_worker(args: IouringWorkerArgs) -> RS<WorkerLoopStats> {
    let recovery_coordinator_for_failure = args.recovery_coordinator.clone();
    let result = block_on_iouring_worker(args);
    if result.is_err() {
        recovery_coordinator_for_failure.worker_failed();
    }
    result
}

fn block_on_iouring_worker(args: IouringWorkerArgs) -> RS<WorkerLoopStats> {
    let runtime = CurrentThreadTaskRuntime::new().map_err(|e| {
        mudu_error!(
            ErrorCode::Tokio,
            "create runtime for io_uring worker error",
            e
        )
    })?;
    // The worker runs a synchronous service loop inside
    // `block_on`, and polls connection/system tasks manually
    // via the worker task registry. Tokio's coop budget is
    // established once for the root poll and is only
    // replenished by the tokio scheduler, which never runs
    // here — after ~128 budget-consuming ops (e.g. tokio
    // Mutex::lock inside notify_wait) `poll_proceed` defers
    // the task's waker to the never-driven runtime and the
    // task parks forever. Unconstrained opts the whole worker
    // out of cooperative scheduling.
    runtime.block_on(mudu_sys::tokio::task::unconstrained(
        run_iouring_worker_async(args),
    ))
}

async fn run_iouring_worker_async(args: IouringWorkerArgs) -> RS<WorkerLoopStats> {
    let IouringWorkerArgs {
        worker_id,
        listen_addr,
        listener,
        sys,
        mailbox_fd,
        mailbox,
        all_mailboxes,
        all_mailbox_fds,
        conn_id_alloc,
        worker_identity,
        worker_count,
        data_dir,
        log_dir,
        log_chunk_size,
        log_batching,
        wal_sync_policy,
        procedure_runtime,
        worker_registry,
        server_instance_id,
        async_runtime,
        stop,
        recovery_coordinator,
    } = args;
    let listener_fd = obtain_iouring_listener_fd(listener, &sys, listen_addr).await?;
    // Create the worker-local io_uring ring up front so
    // that worker initialization (meta catalog, WAL tail
    // scan) can use the io_uring AsyncFs. We drive the ring
    // while creating the worker, then hand both to the loop.
    let mut ring = WorkerRingLoop::new_ring_for_worker(worker_id)?;
    #[allow(clippy::arc_with_non_send_sync)]
    let worker_local_ring = Arc::new(WorkerLocalRing::new_with_task_wake_fd(Some(mailbox_fd)));
    set_current_worker_ring(worker_local_ring.clone());
    let worker_fut = WorkerRuntime::new(WorkerRuntimeParams {
        identity: worker_identity,
        worker_count,
        log_dir,
        data_dir,
        log_chunk_size,
        log_batching,
        wal_sync_policy,
        procedure_runtime,
        registry: worker_registry,
        async_runtime,
        server_instance_id,
        repl_role: ReplRole::Primary,
        repl_leader_hint: None,
        raft_share: None,
        archive_dir: None,
    });
    let worker = match drive_future_with_ring(
        &mut ring,
        &worker_local_ring,
        worker_fut,
        "worker creation",
    ) {
        Ok(worker) => worker,
        Err(e) => {
            mudu_sys::io::worker_ring::unset_current_worker_ring();
            return Err(e);
        }
    };
    let mut loop_state = WorkerRingLoop::new_with_ring(WorkerRingLoopWithRingArgs {
        worker,
        listener_fd,
        mailbox_fd,
        mailbox,
        mailboxes: all_mailboxes,
        mailbox_fds: all_mailbox_fds,
        conn_id_alloc,
        recovery_coordinator,
        stop,
        ring,
        worker_local_ring,
    })?;
    loop_state.initialize().await?;
    loop_state.run().await
}

/// Resolves the worker listener fd: the prebound listener when the serve
/// thread bound one, otherwise a freshly bound socket whose fd is duplicated
/// so the async listener object and the ring loop own independent fds.
async fn obtain_iouring_listener_fd(
    listener: Option<StdTcpListener>,
    sys: &SysIoContext,
    listen_addr: std::net::SocketAddr,
) -> RS<RawFd> {
    match listener {
        Some(std_listener) => Ok(std_listener.into_raw_fd()),
        None => {
            let rt = sys.provider_arc();
            let l = rt.net().bind_tcp(listen_addr).await?;
            let fd = l.as_raw_fd().ok_or_else(|| {
                mudu_error!(ErrorCode::Internal, "async listener does not expose raw fd")
            })?;
            let dup_fd = unsafe { libc::dup(fd) };
            if dup_fd < 0 {
                return Err(mudu_error!(
                    ErrorCode::Network,
                    "dup listener fd error",
                    std::io::Error::last_os_error()
                ));
            }
            Ok(dup_fd)
        }
    }
}

/// Joins every worker thread: a join failure propagates immediately, while
/// per-worker results are folded into the stats list and the first error.
fn join_iouring_workers(
    handles: Vec<SJoinHandle<RS<WorkerLoopStats>>>,
) -> RS<(Vec<WorkerLoopStats>, Option<mudu::error::MuduError>)> {
    let mut worker_stats = Vec::<WorkerLoopStats>::with_capacity(handles.len());
    let mut first_error: Option<mudu::error::MuduError> = None;
    for handle in handles {
        let result = handle
            .join()
            .map_err(|_| mudu_error!(ErrorCode::Thread, "join io_uring worker error"))?;
        match result {
            Ok(stats) => {
                worker_stats.push(stats);
            }
            Err(e) => {
                tracing::error!("io_uring worker error, {}", e);
                if first_error.is_none() {
                    first_error = Some(e);
                }
            }
        }
    }
    Ok((worker_stats, first_error))
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

fn create_mailbox_event_fd() -> RS<RawFd> {
    create_event_fd("create io_uring worker mailbox eventfd error")
}

fn create_event_fd(message: &str) -> RS<RawFd> {
    mudu_sys::sync::blocking::eventfd().map_err(|e| mudu_error!(ErrorCode::Network, message, e))
}

pub(super) fn notify_mailbox_fd(fd: RawFd) -> RS<()> {
    debug!(fd, "server_iouring notify mailbox fd");
    notify_event_fd(fd, "write io_uring worker mailbox eventfd error")
}

fn notify_event_fd(fd: RawFd, message: &str) -> RS<()> {
    mudu_sys::sync::blocking::notify_eventfd(fd)
        .map_err(|e| mudu_error!(ErrorCode::Network, message, e))
}

fn log_worker_stats(stats: &[WorkerLoopStats]) {
    for stat in stats {
        debug!(
            "iouring worker stats: \n\
            worker={}, submit_calls={}, wait_cqe_calls={}, \n\
            accept_submit={}, mailbox_submit={}, recv_submit={}, send_submit={}, \
            log_write_submit={}, cqe_accept={}, cqe_mailbox={}, cqe_recv={}, cqe_send={}, \
            cqe_log_write={}, cqe_close={}, recv_queue_push={}, recv_queue_pop={}, \
            send_queue_push={}, send_queue_pop={}, mailbox_drained={}, local_register={}",
            stat.worker_id,
            stat.submit_calls,
            stat.wait_cqe_calls,
            stat.accept_submit,
            stat.mailbox_submit,
            stat.recv_submit,
            stat.send_submit,
            stat.log_write_submit,
            stat.cqe_accept,
            stat.cqe_mailbox,
            stat.cqe_recv,
            stat.cqe_send,
            stat.cqe_log_write,
            stat.cqe_close,
            stat.recv_queue_push,
            stat.recv_queue_pop,
            stat.send_queue_push,
            stat.send_queue_pop,
            stat.mailbox_drained,
            stat.local_register,
        );
    }
}

pub(crate) fn sockaddr_to_socket_addr(
    storage: &mudu_sys::io::iouring::SockAddrBuf,
) -> RS<std::net::SocketAddr> {
    mudu_sys::io::net::sockaddr_to_socket_addr(storage)
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::todo,
        clippy::unimplemented
    )]

    use super::*;

    #[test]
    fn mailbox_eventfd_accumulates_wakeups() {
        let fd = create_mailbox_event_fd().unwrap();
        notify_mailbox_fd(fd).unwrap();
        notify_mailbox_fd(fd).unwrap();

        let value = mudu_sys::sync::blocking::read_eventfd(fd).unwrap();
        assert_eq!(value, 2);

        mudu_sys::sync::blocking::close_fd(fd).unwrap();
    }

    #[test]
    fn mailbox_can_store_shutdown_messages() {
        let mailbox = SegQueue::new();
        mailbox.push(WorkerMailboxMsg::Shutdown);
        assert!(matches!(mailbox.pop(), Some(WorkerMailboxMsg::Shutdown)));
        assert!(mailbox.pop().is_none());
    }
}
