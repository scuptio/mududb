#[cfg(test)]
use crate::server::callback_registry::{
    AsyncCallback, CallbackDomain, CallbackEventKey, CallbackId, CallbackRegistry, CallbackTrigger,
    PendingCallback,
};
use crate::server::connection_worker_task::spawn_connection_worker_task;
use crate::server::inflight_op::{AcceptOp, InflightOp};
use crate::server::loop_mailbox::{
    drain_messages, handle_read_completion, submit_read_if_needed, LoopMailboxSubmitCtx,
};
use crate::server::loop_user_io::{
    handle_completion as handle_user_io_completion, submit as submit_user_io, LoopUserIoCtx,
};
use crate::server::message_bus_api::{
    register_worker_message_bus, set_current_message_bus, unregister_worker_message_bus,
    unset_current_message_bus,
};
use crate::server::message_bus_runtime::WorkerMessageBus;
use crate::server::server_iouring;
use crate::server::server_iouring::RecoveryCoordinator;
use crate::server::session_bound_worker_runtime::{
    as_worker_local_ref, new_session_bound_worker_runtime,
};
use crate::server::task;
use crate::server::worker::WorkerRuntime;
use crate::server::worker_local::{set_current_worker_local, unset_current_worker_local};
use crate::server::worker_loop_stats::WorkerLoopStats;
use crate::server::worker_mailbox::WorkerMailboxMsg;
use mudu_sys::io::worker_ring::{
    set_current_worker_ring, unset_current_worker_ring, WorkerLocalRing,
};

use crate::wal::worker_log::ChunkedWorkerLogBackend;
use crate::wal::xl_batch_worker_log::{new_xl_batch_worker_log, XLBatchWorkerLog};
use crossbeam_queue::SegQueue;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_utils::scoped_task_trace;
use mudu_utils::task_context::TaskContext;
use std::collections::HashMap;

use std::future::Future;
use std::os::fd::RawFd;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use mudu_sys::sync::SMutex;
use std::time::Duration;

#[cfg(test)]
use mudu_sys::net::sync::StdTcpListener;
use tracing::{debug, trace};

#[path = "worker_ring_loop/recovery.rs"]
mod recovery;
#[path = "worker_ring_loop/runtime.rs"]
mod runtime;

type XLWorkerLog =
    XLBatchWorkerLog<ChunkedWorkerLogBackend, recovery::WorkerRingLoopRecoveryHandler>;
/// Drives a single io_uring worker event loop.
///
/// The loop owns the worker-local ring and multiplexes several kinds of work:
/// accepting new sockets, consuming inter-worker mailbox notifications,
/// completing user-triggered file/socket I/O, and coordinating connection task
/// lifecycle. It also performs worker-log recovery before the steady-state loop
/// starts so replayed state is visible to newly accepted connections.
pub(in crate::server) struct WorkerRingLoop {
    worker: WorkerRuntime,
    log: Option<XLWorkerLog>,
    ring: mudu_sys::io::iouring::IoUring,
    listener_fd: RawFd,
    mailbox_fd: RawFd,
    mailbox: Arc<SegQueue<WorkerMailboxMsg>>,
    conn_id_alloc: Arc<AtomicU64>,
    recovery_coordinator: Arc<RecoveryCoordinator>,
    worker_local_ring: Arc<WorkerLocalRing>,
    message_bus: Arc<WorkerMessageBus>,
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
    accept_submitted: bool,
    stop: Arc<AtomicBool>,
    stats: WorkerLoopStats,
}

#[cfg(test)]
pub(in crate::server) struct WorkerRingLoopArgs {
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

pub(in crate::server) struct WorkerRingLoopWithRingArgs {
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
    pub(in crate::server) fn new(args: WorkerRingLoopArgs) -> RS<Self> {
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
        let ring = Self::new_ring()?;
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

    pub(in crate::server) fn new_with_ring(args: WorkerRingLoopWithRingArgs) -> RS<Self> {
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
        let message_bus = WorkerMessageBus::new(
            worker.worker_id(),
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
            message_bus,
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
            accept_submitted: false,
            stop,
            stats: WorkerLoopStats {
                worker_id,
                ..WorkerLoopStats::default()
            },
        })
    }

    pub(in crate::server) fn new_ring() -> RS<mudu_sys::io::iouring::IoUring> {
        let ring = mudu_sys::io::iouring::IoUring::new(1024);
        match ring {
            Ok(ring) => Ok(ring),
            Err(rc) => Err(mudu_error!(
                ErrorCode::Network,
                format!("io_uring_queue_init_params error {}", rc)
            )),
        }
    }

    pub(in crate::server) async fn initialize(&mut self) -> RS<()> {
        trace!(
            worker_id = self.worker.worker_id(),
            "worker_ring_loop run start"
        );
        set_current_worker_local(as_worker_local_ref(new_session_bound_worker_runtime(
            self.worker.clone(),
            0,
        )));
        set_current_message_bus(self.message_bus.as_ref());
        // Install the worker-local ring before any initialization that may use
        // the io_uring-based async file system (e.g. meta catalog open), and
        // drive it while initialization runs so queued io_uring I/O completes.
        set_current_worker_ring(self.worker_local_ring.clone());
        //self.worker.meta_mgr().initialize().await?;
        register_worker_message_bus(
            self.worker.server_instance_id(),
            self.worker.worker_id(),
            &self.message_bus.as_ref(),
        )?;
        self.worker.ensure_partition_rpc_handler()?;
        trace!(
            worker_id = self.worker.worker_id(),
            "worker_ring_loop partition rpc ready"
        );
        trace!(
            worker_id = self.worker.worker_id(),
            "worker_ring_loop bootstrap storage start"
        );
        let worker = self.worker.clone();
        if let Err(e) = self.drive_local_future(worker.initialize(), "worker initialize") {
            unset_current_worker_ring();
            return Err(e);
        }
        Ok(())
    }

    /// Runs worker recovery and then enters the main service loop.
    ///
    /// The worker-local ring pointer is installed for the duration of the run
    /// so user-level async file I/O can enqueue requests onto this loop.
    pub(in crate::server) async fn run(&mut self) -> RS<WorkerLoopStats> {
        scoped_task_trace!();
        set_current_worker_ring(self.worker_local_ring.clone());
        {
            let worker = self.worker.clone();
            let bootstrap_fut = worker.bootstrap_storage_async();
            let mut bootstrap_fut = std::pin::pin!(bootstrap_fut);
            let waker = noop_waker();
            let mut cx = std::task::Context::from_waker(&waker);
            loop {
                match bootstrap_fut.as_mut().poll(&mut cx) {
                    std::task::Poll::Ready(Ok(())) => break,
                    std::task::Poll::Ready(Err(e)) => {
                        let _ = unregister_worker_message_bus(
                            self.worker.server_instance_id(),
                            self.worker.worker_id(),
                        );
                        unset_current_message_bus();
                        unset_current_worker_ring();
                        unset_current_worker_local();
                        self.recovery_coordinator.worker_failed();
                        return Err(e);
                    }
                    std::task::Poll::Pending => {
                        if let Err(e) = self.submit_user_ring_io_if_needed() {
                            let _ = unregister_worker_message_bus(
                                self.worker.server_instance_id(),
                                self.worker.worker_id(),
                            );
                            unset_current_message_bus();
                            unset_current_worker_ring();
                            unset_current_worker_local();
                            self.recovery_coordinator.worker_failed();
                            return Err(e);
                        }
                        let submitted = self.ring.submit();
                        if submitted < 0 {
                            let _ = unregister_worker_message_bus(
                                self.worker.server_instance_id(),
                                self.worker.worker_id(),
                            );
                            unset_current_message_bus();
                            unset_current_worker_ring();
                            unset_current_worker_local();
                            self.recovery_coordinator.worker_failed();
                            return Err(mudu_error!(
                                ErrorCode::Network,
                                format!("io_uring submit error during bootstrap {}", submitted)
                            ));
                        }
                        let cqe = match self.ring.wait() {
                            Ok(cqe) => cqe,
                            Err(wait_rc) => {
                                let _ = unregister_worker_message_bus(
                                    self.worker.server_instance_id(),
                                    self.worker.worker_id(),
                                );
                                unset_current_message_bus();
                                unset_current_worker_ring();
                                unset_current_worker_local();
                                self.recovery_coordinator.worker_failed();
                                return Err(mudu_error!(
                                    ErrorCode::Network,
                                    format!("io_uring wait cqe error during bootstrap {}", wait_rc)
                                ));
                            }
                        };
                        if let Err(e) = self.process_cqe(cqe) {
                            let _ = unregister_worker_message_bus(
                                self.worker.server_instance_id(),
                                self.worker.worker_id(),
                            );
                            unset_current_message_bus();
                            unset_current_worker_ring();
                            unset_current_worker_local();
                            self.recovery_coordinator.worker_failed();
                            return Err(e);
                        }
                    }
                }
            }
        }

        trace!(
            worker_id = self.worker.worker_id(),
            "worker_ring_loop bootstrap storage done"
        );
        // The worker log backend is initialized lazily during initialize().
        // Refresh the ring loop's local log wrapper so recovery and steady-
        // state flush polling use the real backend instead of the placeholder
        // captured in WorkerRingLoop::new before initialization.
        self.log = self.worker.worker_log()?.map(|backend| {
            new_xl_batch_worker_log(
                backend.clone(),
                recovery::WorkerRingLoopRecoveryHandler {
                    worker: self.worker.clone(),
                },
            )
        });
        if let Err(err) = self.recover_worker_log_on_loop() {
            let _ = unregister_worker_message_bus(
                self.worker.server_instance_id(),
                self.worker.worker_id(),
            );
            unset_current_message_bus();
            unset_current_worker_ring();
            unset_current_worker_local();
            self.recovery_coordinator.worker_failed();
            return Err(err);
        }
        self.recovery_coordinator.worker_succeeded()?;
        trace!(
            worker_id = self.worker.worker_id(),
            "worker_ring_loop recovery barrier passed"
        );
        let worker = self.worker.clone();
        self.spawn(None, async move {
            worker.recover_cross_partition_transactions().await
        });
        let r = self.run_service_loop();
        let _ = unregister_worker_message_bus(
            self.worker.server_instance_id(),
            self.worker.worker_id(),
        );
        unset_current_message_bus();
        unset_current_worker_ring();
        unset_current_worker_local();
        r
    }

    pub(in crate::server) fn spawn(
        &self,
        conn_id: Option<u64>,
        future: impl Future<Output = RS<()>> + 'static,
    ) {
        self.worker_local_ring
            .worker_task_registry()
            .spawn(conn_id, Box::pin(future));
    }

    fn drive_local_future<F, T>(&mut self, future: F, phase: &str) -> RS<T>
    where
        F: Future<Output = RS<T>>,
    {
        let mut future = std::pin::pin!(future);
        let waker = noop_waker();
        let mut cx = std::task::Context::from_waker(&waker);
        loop {
            match future.as_mut().poll(&mut cx) {
                std::task::Poll::Ready(result) => return result,
                std::task::Poll::Pending => {
                    self.submit_user_ring_io_if_needed()?;
                    let submitted = self.ring.submit();
                    if submitted < 0 {
                        return Err(mudu_error!(
                            ErrorCode::Network,
                            format!("io_uring submit error during {} {}", phase, submitted)
                        ));
                    }
                    let cqe = self.ring.wait().map_err(|wait_rc| {
                        mudu_error!(
                            ErrorCode::Network,
                            format!("io_uring wait cqe error during {} {}", phase, wait_rc)
                        )
                    })?;
                    self.process_cqe(cqe)?;
                }
            }
        }
    }

    pub(in crate::server) fn process_cqe(&mut self, cqe: mudu_sys::io::iouring::Cqe) -> RS<()> {
        let token = cqe.user_data();
        let result = cqe.result();
        let op = self.inflight.remove(&token).ok_or_else(|| {
            mudu_error!(
                ErrorCode::Internal,
                format!("unknown io_uring completion token {}", token)
            )
        })?;

        // Completion dispatch is token-based: each submitted SQE inserts a
        // matching inflight entry, and the CQE result is routed here.
        match op {
            InflightOp::Accept(op) => {
                self.stats.cqe_accept += 1;
                self.accept_submitted = false;
                if result >= 0 {
                    let conn_fd = result as RawFd;
                    let remote_addr = server_iouring::sockaddr_to_socket_addr(op.addr())?;
                    mudu_sys::io::net::set_tcp_nodelay(conn_fd)?;
                    let conn_id = self.conn_id_alloc.fetch_add(1, Ordering::Relaxed);
                    self.register_connection(conn_id, conn_fd, remote_addr)?;
                }
            }
            InflightOp::MailboxRead { .. } => {
                debug!(
                    worker_id = self.worker.worker_id(),
                    mailbox_fd = self.mailbox_fd,
                    result,
                    "worker_ring_loop mailbox read cqe"
                );
                handle_read_completion(
                    self.worker.worker_id(),
                    self.mailbox_fd,
                    &mut self.mailbox_read_submitted,
                    &mut self.stats,
                );
                for msg in drain_messages(self.mailbox.as_ref(), &mut self.stats) {
                    self.handle_mailbox_message(msg)?;
                }
            }
            InflightOp::UserIo(op) => {
                let op_id = op.op_id();
                let op_kind = op.kind();
                if let Some(task_id) = self.worker_local_ring.task_for_op(op_id) {
                    if let Some(ctx) = TaskContext::get(task_id) {
                        ctx.watch("io.last_op_id", &op_id.to_string());
                        ctx.watch("io.last_op_kind", op_kind);
                        ctx.watch("io.last_cqe_token", &token.to_string());
                        ctx.watch("io.last_result", &result.to_string());
                    }
                }
                handle_user_io_completion(&self.worker_local_ring, op, result)?
            }
        }
        Ok(())
    }

    fn handle_mailbox_message(&self, msg: WorkerMailboxMsg) -> RS<()> {
        match msg {
            WorkerMailboxMsg::BusMessage(envelope) => {
                debug!(
                    worker_id = self.worker.worker_id(),
                    src = ?envelope.src(),
                    dst = ?envelope.dst(),
                    kind = ?envelope.kind(),
                    msg_id = envelope.msg_id(),
                    correlation_id = ?envelope.correlation_id(),
                    "worker_ring_loop received bus mailbox message"
                );
                self.message_bus.handle_incoming(envelope)?;
                debug!(
                    worker_id = self.worker.worker_id(),
                    "worker_ring_loop handled bus mailbox message"
                );
            }
            WorkerMailboxMsg::Shutdown => {
                debug!(
                    worker_id = self.worker.worker_id(),
                    "worker_ring_loop received shutdown mailbox message"
                );
                self.shutdown_triggered.store(true, Ordering::Relaxed);
            }
        }
        Ok(())
    }

    pub(in crate::server) fn register_connection(
        &mut self,
        conn_id: u64,
        fd: RawFd,
        remote_addr: std::net::SocketAddr,
    ) -> RS<()> {
        self.stats.local_register += 1;
        self.start_connection_task(conn_id, fd, remote_addr, None)
    }

    pub(in crate::server) fn submit_accept_if_needed(&mut self) -> RS<()> {
        if self.shutting_down || self.accept_submitted || self.listener_fd < 0 {
            return Ok(());
        }
        let token = self.alloc_token();
        let Some(mut sqe) = self.ring.next_sqe() else {
            return Ok(());
        };
        let mut op = Box::new(AcceptOp::new(
            mudu_sys::io::iouring::SockAddrBuf::new_empty(),
        ));
        sqe.set_user_data(token);
        sqe.prep_accept(self.listener_fd, op.addr_mut(), 0);
        self.inflight.insert(token, InflightOp::Accept(op));
        self.accept_submitted = true;
        self.stats.accept_submit += 1;
        Ok(())
    }

    pub(in crate::server) fn submit_mailbox_read_if_needed(&mut self) -> RS<()> {
        debug!(
            worker_id = self.worker.worker_id(),
            mailbox_fd = self.mailbox_fd,
            mailbox_read_submitted = self.mailbox_read_submitted,
            shutting_down = self.shutting_down,
            "worker_ring_loop submit_mailbox_read_if_needed"
        );
        let mut ctx = LoopMailboxSubmitCtx {
            worker_id: self.worker.worker_id(),
            ring: &mut self.ring,
            mailbox_fd: self.mailbox_fd,
            mailbox_read_submitted: &mut self.mailbox_read_submitted,
            inflight: &mut self.inflight,
            next_token: &mut self.next_token,
            stats: &mut self.stats,
            shutting_down: self.shutting_down,
        };
        submit_read_if_needed(&mut ctx)
    }

    pub(in crate::server) fn submit_user_ring_io_if_needed(&mut self) -> RS<()> {
        let mut ctx = LoopUserIoCtx {
            ring: &mut self.ring,
            user_ring: &self.worker_local_ring,
            inflight: &mut self.inflight,
            next_token: &mut self.next_token,
        };
        submit_user_io(&mut ctx)
    }

    pub(in crate::server) fn alloc_token(&mut self) -> u64 {
        let token = self.next_token;
        self.next_token += 1;
        token
    }

    fn start_connection_task(
        &self,
        conn_id: u64,
        fd: RawFd,
        remote_addr: std::net::SocketAddr,
        initial_response: Option<Vec<u8>>,
    ) -> RS<()> {
        let socket = mudu_sys::io::socket::IoSocket::from_raw_fd(fd);
        let _ = self.connection_task_fds.lock()?.insert(conn_id, fd);
        task::spawn(
            Some(conn_id),
            spawn_connection_worker_task(
                self.worker.clone(),
                self.connection_task_fds.clone(),
                conn_id,
                socket,
                remote_addr,
                initial_response,
            ),
        );
        Ok(())
    }

    #[cfg(test)]
    pub(in crate::server) fn register_async_callback(
        &mut self,
        trigger: CallbackTrigger,
        callback: AsyncCallback,
    ) -> RS<CallbackId> {
        if let CallbackTrigger::Sequence { domain, target } = trigger {
            if let Some(frontier) = self.callback_sequence_frontiers.get(&domain).copied() {
                if frontier >= target {
                    let id = self.callback_registry.register(trigger, callback);
                    let ready = self.callback_registry.advance_sequence(domain, frontier);
                    self.spawn_ready_callbacks(ready)?;
                    return Ok(id);
                }
            }
        }
        Ok(self.callback_registry.register(trigger, callback))
    }

    #[cfg(test)]
    pub(in crate::server) fn cancel_async_callback(&mut self, callback_id: CallbackId) -> bool {
        self.callback_registry.cancel(callback_id)
    }

    #[cfg(test)]
    pub(in crate::server) fn fire_callback_event(&mut self, key: CallbackEventKey) -> RS<()> {
        let ready = self.callback_registry.fire_event(key);
        self.spawn_ready_callbacks(ready)
    }

    #[cfg(test)]
    pub(in crate::server) fn advance_callback_sequence(
        &mut self,
        domain: CallbackDomain,
        value: u64,
    ) -> RS<()> {
        let frontier = self.callback_sequence_frontiers.entry(domain).or_insert(0);
        if value <= *frontier {
            return Ok(());
        }
        *frontier = value;
        let ready = self.callback_registry.advance_sequence(domain, value);
        self.spawn_ready_callbacks(ready)
    }

    #[cfg(test)]
    fn spawn_ready_callbacks(&mut self, callbacks: Vec<PendingCallback>) -> RS<()> {
        for pending in callbacks {
            let future = (pending.callback)();
            self.spawn(None, future);
        }
        Ok(())
    }
}

/// Drive `future` to completion using a standalone worker ring. This is used
/// before `WorkerRingLoop` is fully constructed, so that worker initialization
/// (which may open meta catalogs via the io_uring AsyncFs) can submit and
/// complete io_uring I/O.
pub(in crate::server) fn drive_future_with_ring<F, T>(
    ring: &mut mudu_sys::io::iouring::IoUring,
    worker_local_ring: &Arc<WorkerLocalRing>,
    future: F,
    phase: &str,
) -> RS<T>
where
    F: Future<Output = RS<T>>,
{
    let mut inflight = HashMap::<u64, InflightOp>::new();
    let mut next_token: u64 = 1;
    let mut future = std::pin::pin!(future);
    let waker = noop_waker();
    let mut cx = std::task::Context::from_waker(&waker);
    loop {
        match future.as_mut().poll(&mut cx) {
            std::task::Poll::Ready(result) => return result,
            std::task::Poll::Pending => {
                let mut ctx = LoopUserIoCtx {
                    ring,
                    user_ring: worker_local_ring,
                    inflight: &mut inflight,
                    next_token: &mut next_token,
                };
                submit_user_io(&mut ctx)?;
                let submitted = ring.submit();
                if submitted < 0 {
                    return Err(mudu_error!(
                        ErrorCode::Network,
                        format!("io_uring submit error during {} {}", phase, submitted)
                    ));
                }
                let cqe = ring.wait().map_err(|wait_rc| {
                    mudu_error!(
                        ErrorCode::Network,
                        format!("io_uring wait cqe error during {} {}", phase, wait_rc)
                    )
                })?;
                let token = cqe.user_data();
                let result = cqe.result();
                let op = inflight.remove(&token).ok_or_else(|| {
                    mudu_error!(
                        ErrorCode::Internal,
                        format!("unknown io_uring completion token {}", token)
                    )
                })?;
                match op {
                    InflightOp::UserIo(op) => {
                        handle_user_io_completion(worker_local_ring, op, result)?
                    }
                    _other => {
                        return Err(mudu_error!(
                            ErrorCode::Internal,
                            format!("unexpected non-user io completion during {}", phase)
                        ));
                    }
                }
            }
        }
    }
}

fn noop_waker() -> std::task::Waker {
    use std::task::{RawWaker, RawWakerVTable, Waker};
    unsafe fn noop_clone(_: *const ()) -> RawWaker {
        noop_raw_waker()
    }
    unsafe fn noop(_: *const ()) {}
    const VTABLE: RawWakerVTable = RawWakerVTable::new(noop_clone, noop, noop, noop);
    fn noop_raw_waker() -> RawWaker {
        RawWaker::new(std::ptr::null(), &VTABLE)
    }
    unsafe { Waker::from_raw(noop_raw_waker()) }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::todo,
        clippy::unimplemented
    )]

    use super::*;
    use crate::server::callback_registry::{CallbackDomain, CallbackEventKey, CallbackTrigger};
    use crate::server::worker::WorkerRuntimeParams;
    use crate::server::worker_registry::load_or_create_worker_registry;
    use crate::wal::worker_log::WorkerLogBatching;
    use mudu_sys::env_var::temp_dir;
    use mudu_sys::imp::native::linux::io_uring::file::{close, flush, open, read, write};
    use mudu_sys::io::socket::{
        accept, close as close_socket, connect, recv, send, shutdown, socket, IoSocket,
    };
    use mudu_sys::tokio::task::yield_now;
    use mudu_sys::TaskJoinHandle;
    use mudu_utils::oid::gen_oid;
    use mudu_utils::task_async::spawn_task_detached;
    use std::io::{Read, Write};
    use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};

    async fn test_worker_loop() -> Option<WorkerRingLoop> {
        let dir = temp_dir()
            .join(format!("worker_ring_loop_test_{}", gen_oid()))
            .to_string_lossy()
            .into_owned();
        let registry = load_or_create_worker_registry(&dir, 1).unwrap();
        let identity = registry.worker(0).cloned().unwrap();
        let worker = WorkerRuntime::new(WorkerRuntimeParams {
            identity,
            worker_count: 1,
            log_dir: dir.clone(),
            data_dir: dir.clone(),
            log_chunk_size: 4096,
            log_batching: WorkerLogBatching::default(),
            procedure_runtime: None,
            registry,
            async_runtime: None,
            server_instance_id: 0,
        })
        .await
        .unwrap();
        let mailbox_fd = mudu_sys::sync::sync_::blocking::eventfd().unwrap();
        match WorkerRingLoop::new(WorkerRingLoopArgs {
            worker,
            listener_fd: -1,
            mailbox_fd,
            mailbox: Arc::new(SegQueue::new()),
            mailboxes: vec![Arc::new(SegQueue::new())],
            mailbox_fds: vec![mailbox_fd],
            conn_id_alloc: Arc::new(AtomicU64::new(1)),
            recovery_coordinator: Arc::new(RecoveryCoordinator::new(1, None)),
            stop: Arc::new(AtomicBool::new(false)),
        }) {
            Ok(loop_state) => Some(loop_state),
            Err(_) => {
                let _ = mudu_sys::sync::sync_::blocking::close_fd(mailbox_fd);
                None
            }
        }
    }

    async fn drive_ring_future<T>(
        loop_state: &mut WorkerRingLoop,
        handle: &TaskJoinHandle<Option<T>>,
    ) -> RS<()>
    where
        T: Send + 'static,
    {
        while !handle.is_finished() {
            loop_state.submit_user_ring_io_if_needed()?;
            let submitted = loop_state.ring.submit();
            if submitted < 0 {
                return Err(mudu_error!(
                    ErrorCode::Network,
                    format!("io_uring_submit error {}", submitted)
                ));
            }
            if loop_state.inflight.is_empty() {
                yield_now().await;
                continue;
            }
            let cqe = loop_state.ring.wait().map_err(|wait_rc| {
                mudu_error!(
                    ErrorCode::Network,
                    format!("io_uring_wait_cqe error {}", wait_rc)
                )
            })?;
            loop_state.process_cqe(cqe)?;
            yield_now().await;
        }
        Ok(())
    }

    // These tests drive the real Linux io_uring backend, which Miri cannot
    // emulate (it does not support the io_uring syscalls/FFI). They are
    // ignored under Miri and run only on native Linux builds.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn worker_ring_loop_executes_user_file_io_via_cqe() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let Some(mut loop_state) = test_worker_loop().await else {
                return;
            };
            set_current_worker_ring(loop_state.worker_local_ring.clone());

            let path = temp_dir().join(format!("iouring_file_io_{}", gen_oid()));
            let path_str = path.to_string_lossy().into_owned();

            let open_task = spawn_task_detached("test", {
                let path_str = path_str.clone();
                async move {
                    open(
                        &path_str,
                        libc::O_CREAT | libc::O_RDWR | libc::O_TRUNC | libc::O_CLOEXEC,
                        0o644,
                    )
                    .await
                }
            })
            .unwrap();
            yield_now().await;
            drive_ring_future(&mut loop_state, &open_task)
                .await
                .unwrap();
            let file = open_task.await.unwrap().unwrap().unwrap();

            let fd = file.fd();
            let write_task = spawn_task_detached("test", async move {
                write(
                    &mudu_sys::imp::native::linux::io_uring::file::IoFile::from_raw_fd(fd),
                    b"hello iouring".to_vec(),
                    0,
                )
                .await
            })
            .unwrap();
            yield_now().await;
            drive_ring_future(&mut loop_state, &write_task)
                .await
                .unwrap();
            assert_eq!(
                write_task.await.unwrap().unwrap().unwrap(),
                b"hello iouring".len()
            );

            let fd = file.fd();
            let flush_task = spawn_task_detached("test", async move {
                flush(&mudu_sys::imp::native::linux::io_uring::file::IoFile::from_raw_fd(fd)).await
            })
            .unwrap();
            yield_now().await;
            drive_ring_future(&mut loop_state, &flush_task)
                .await
                .unwrap();
            flush_task.await.unwrap().unwrap().unwrap();

            let fd = file.fd();
            let read_task = spawn_task_detached("test", async move {
                read(
                    &mudu_sys::imp::native::linux::io_uring::file::IoFile::from_raw_fd(fd),
                    13,
                    0,
                )
                .await
            })
            .unwrap();
            yield_now().await;
            drive_ring_future(&mut loop_state, &read_task)
                .await
                .unwrap();
            assert_eq!(
                read_task.await.unwrap().unwrap().unwrap(),
                b"hello iouring".to_vec()
            );

            let close_task = spawn_task_detached("test", async move { close(file).await }).unwrap();
            yield_now().await;
            drive_ring_future(&mut loop_state, &close_task)
                .await
                .unwrap();
            close_task.await.unwrap().unwrap().unwrap();

            unset_current_worker_ring();
            loop_state.ring.exit();
            mudu_sys::sync::sync_::blocking::close_fd(loop_state.mailbox_fd).unwrap();
            let _ = mudu_sys::fs::sync::remove_file(&path);
        })
        .unwrap()
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn worker_ring_loop_executes_user_socket_connect_io_via_cqe() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let Some(mut loop_state) = test_worker_loop().await else {
                return;
            };
            set_current_worker_ring(loop_state.worker_local_ring.clone());

            let listener = StdTcpListener::bind("127.0.0.1:0".parse().unwrap()).unwrap();
            let addr = listener.local_addr().unwrap();
            let peer = mudu_sys::task::sync::spawn_thread(move || -> RS<()> {
                let (mut stream, _) = listener.accept().unwrap();
                let mut buf = [0u8; 4];
                stream.read_exact(&mut buf).unwrap();
                assert_eq!(&buf, b"ping");
                stream.write_all(b"pong").unwrap();
                let mut eof = [0u8; 1];
                let read = stream.read(&mut eof).unwrap();
                assert_eq!(read, 0);
                Ok(())
            })
            .unwrap();

            let socket_task = spawn_task_detached("test", async {
                socket(libc::AF_INET, libc::SOCK_STREAM | libc::SOCK_CLOEXEC, 0).await
            })
            .unwrap();
            yield_now().await;
            drive_ring_future(&mut loop_state, &socket_task)
                .await
                .unwrap();
            let sock = socket_task.await.unwrap().unwrap().unwrap();

            let fd = sock.fd();
            let connect_task = spawn_task_detached("test", async move {
                connect(&IoSocket::from_raw_fd(fd), addr).await
            })
            .unwrap();
            yield_now().await;
            drive_ring_future(&mut loop_state, &connect_task)
                .await
                .unwrap();
            connect_task.await.unwrap().unwrap().unwrap();

            let fd = sock.fd();
            let send_task = spawn_task_detached("test", async move {
                send(&IoSocket::from_raw_fd(fd), b"ping".to_vec(), 0).await
            })
            .unwrap();
            yield_now().await;
            drive_ring_future(&mut loop_state, &send_task)
                .await
                .unwrap();
            assert_eq!(send_task.await.unwrap().unwrap().unwrap(), 4);

            let fd = sock.fd();
            let recv_task = spawn_task_detached("test", async move {
                recv(&IoSocket::from_raw_fd(fd), 4, 0).await
            })
            .unwrap();
            yield_now().await;
            drive_ring_future(&mut loop_state, &recv_task)
                .await
                .unwrap();
            assert_eq!(recv_task.await.unwrap().unwrap().unwrap(), b"pong".to_vec());

            let fd = sock.fd();
            let shutdown_task = spawn_task_detached("test", async move {
                shutdown(&IoSocket::from_raw_fd(fd), libc::SHUT_WR).await
            })
            .unwrap();
            yield_now().await;
            drive_ring_future(&mut loop_state, &shutdown_task)
                .await
                .unwrap();
            shutdown_task.await.unwrap().unwrap().unwrap();

            let close_task =
                spawn_task_detached("test", async move { close_socket(sock).await }).unwrap();
            yield_now().await;
            drive_ring_future(&mut loop_state, &close_task)
                .await
                .unwrap();
            close_task.await.unwrap().unwrap().unwrap();

            peer.join().unwrap().unwrap();

            unset_current_worker_ring();
            loop_state.ring.exit();
            mudu_sys::sync::sync_::blocking::close_fd(loop_state.mailbox_fd).unwrap();
        })
        .unwrap()
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn worker_ring_loop_executes_user_socket_accept_io_via_cqe() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let Some(mut loop_state) = test_worker_loop().await else {
                return;
            };
            set_current_worker_ring(loop_state.worker_local_ring.clone());

            let listener = StdTcpListener::bind("127.0.0.1:0".parse().unwrap()).unwrap();
            let addr = listener.local_addr().unwrap();
            let listener_fd = unsafe { libc::dup(listener.into_raw_fd()) };
            assert!(listener_fd >= 0);
            let listener_sock = IoSocket::from_raw_fd(listener_fd);

            let peer = mudu_sys::task::sync::spawn_thread(move || -> RS<()> {
                let mut stream = mudu_sys::net::sync::connect_tcp(addr).unwrap();
                stream.write_all(b"ping").unwrap();
                let mut buf = [0u8; 4];
                stream.read_exact(&mut buf).unwrap();
                assert_eq!(&buf, b"pong");
                Ok(())
            })
            .unwrap();

            let accept_fd = listener_sock.fd();
            let accept_task = spawn_task_detached("test", async move {
                accept(&IoSocket::from_raw_fd(accept_fd)).await
            })
            .unwrap();
            yield_now().await;
            drive_ring_future(&mut loop_state, &accept_task)
                .await
                .unwrap();
            let (accepted, remote_addr) = accept_task.await.unwrap().unwrap().unwrap();
            assert_eq!(
                remote_addr.ip(),
                std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST)
            );

            let accepted_fd = accepted.fd();
            let recv_task = spawn_task_detached("test", async move {
                recv(&IoSocket::from_raw_fd(accepted_fd), 4, 0).await
            })
            .unwrap();
            yield_now().await;
            drive_ring_future(&mut loop_state, &recv_task)
                .await
                .unwrap();
            assert_eq!(recv_task.await.unwrap().unwrap().unwrap(), b"ping".to_vec());

            let accepted_fd = accepted.fd();
            let send_task = spawn_task_detached("test", async move {
                send(&IoSocket::from_raw_fd(accepted_fd), b"pong".to_vec(), 0).await
            })
            .unwrap();
            yield_now().await;
            drive_ring_future(&mut loop_state, &send_task)
                .await
                .unwrap();
            assert_eq!(send_task.await.unwrap().unwrap().unwrap(), 4);

            let accepted_fd = accepted.fd();
            let shutdown_task = spawn_task_detached("test", async move {
                shutdown(&IoSocket::from_raw_fd(accepted_fd), libc::SHUT_WR).await
            })
            .unwrap();
            yield_now().await;
            drive_ring_future(&mut loop_state, &shutdown_task)
                .await
                .unwrap();
            shutdown_task.await.unwrap().unwrap().unwrap();

            let close_accepted_task =
                spawn_task_detached("test", async move { close_socket(accepted).await }).unwrap();
            yield_now().await;
            drive_ring_future(&mut loop_state, &close_accepted_task)
                .await
                .unwrap();
            close_accepted_task.await.unwrap().unwrap().unwrap();

            let close_listener_task =
                spawn_task_detached("test", async move { close_socket(listener_sock).await })
                    .unwrap();
            yield_now().await;
            drive_ring_future(&mut loop_state, &close_listener_task)
                .await
                .unwrap();
            close_listener_task.await.unwrap().unwrap().unwrap();

            peer.join().unwrap().unwrap();

            unset_current_worker_ring();
            loop_state.ring.exit();
            mudu_sys::sync::sync_::blocking::close_fd(loop_state.mailbox_fd).unwrap();
        })
        .unwrap()
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn worker_ring_loop_runs_event_callback_as_system_task() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let Some(mut loop_state) = test_worker_loop().await else {
                return;
            };
            let hit = Arc::new(AtomicUsize::new(0));
            let hit_clone = hit.clone();
            let callback_id = loop_state
                .register_async_callback(
                    CallbackTrigger::Event(CallbackEventKey { kind: 7, id: 99 }),
                    Box::new(move || {
                        Box::pin(async move {
                            hit_clone.fetch_add(1, AtomicOrdering::SeqCst);
                            Ok(())
                        })
                    }),
                )
                .unwrap();
            assert!(callback_id > 0);

            loop_state
                .fire_callback_event(CallbackEventKey { kind: 7, id: 99 })
                .unwrap();
            loop_state.poll_ready_worker_tasks().unwrap();
            assert_eq!(hit.load(AtomicOrdering::SeqCst), 1);

            loop_state.ring.exit();
            mudu_sys::sync::sync_::blocking::close_fd(loop_state.mailbox_fd).unwrap();
        })
        .unwrap()
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn worker_ring_loop_runs_sequence_callback_when_frontier_advances_and_skips_cancelled() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let Some(mut loop_state) = test_worker_loop().await else {
                return;
            };
            let hit = Arc::new(AtomicUsize::new(0));

            let first_hit = hit.clone();
            loop_state
                .register_async_callback(
                    CallbackTrigger::Sequence {
                        domain: CallbackDomain::Generic(3),
                        target: 4,
                    },
                    Box::new(move || {
                        Box::pin(async move {
                            first_hit.fetch_add(1, AtomicOrdering::SeqCst);
                            Ok(())
                        })
                    }),
                )
                .unwrap();

            let cancelled_hit = hit.clone();
            let cancelled = loop_state
                .register_async_callback(
                    CallbackTrigger::Sequence {
                        domain: CallbackDomain::Generic(3),
                        target: 5,
                    },
                    Box::new(move || {
                        Box::pin(async move {
                            cancelled_hit.fetch_add(100, AtomicOrdering::SeqCst);
                            Ok(())
                        })
                    }),
                )
                .unwrap();
            assert!(loop_state.cancel_async_callback(cancelled));

            loop_state
                .advance_callback_sequence(CallbackDomain::Generic(3), 4)
                .unwrap();
            loop_state.poll_ready_worker_tasks().unwrap();
            assert_eq!(hit.load(AtomicOrdering::SeqCst), 1);

            loop_state
                .advance_callback_sequence(CallbackDomain::Generic(3), 5)
                .unwrap();
            loop_state.poll_ready_worker_tasks().unwrap();
            assert_eq!(hit.load(AtomicOrdering::SeqCst), 1);

            loop_state.ring.exit();
            mudu_sys::sync::sync_::blocking::close_fd(loop_state.mailbox_fd).unwrap();
        })
        .unwrap()
    }
}
