use crate::io::worker_ring::{set_current_worker_ring, unset_current_worker_ring, WorkerLocalRing};
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
use crate::server::worker_task::{spawn_system_worker_task, WorkerTaskFuture};
use crate::wal::worker_log::ChunkedWorkerLogBackend;
use crate::wal::xl_batch_worker_log::{new_xl_batch_worker_log, XLBatchWorkerLog};
use crossbeam_queue::SegQueue;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use mudu_utils::scoped_task_trace;
use mudu_utils::task_context::TaskContext;
use std::collections::HashMap;
use std::fs::OpenOptions;
use std::os::fd::{AsRawFd, RawFd};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
#[cfg(test)]
use std::thread;
use std::time::Duration;
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
    ring: mudu_sys::uring::IoUring,
    listener_fd: RawFd,
    mailbox_fd: RawFd,
    mailbox: Arc<SegQueue<WorkerMailboxMsg>>,
    conn_id_alloc: Arc<AtomicU64>,
    recovery_coordinator: Arc<RecoveryCoordinator>,
    worker_local_ring: Arc<WorkerLocalRing>,
    message_bus: Arc<WorkerMessageBus>,
    connection_task_fds: Arc<scc::HashMap<u64, RawFd>>,
    #[allow(dead_code)]
    callback_registry: CallbackRegistry,
    #[allow(dead_code)]
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

impl WorkerRingLoop {
    /// Builds the runtime state for one worker loop and initializes its
    /// private io_uring instance.
    pub(in crate::server) fn new(
        worker: WorkerRuntime,
        listener_fd: RawFd,
        mailbox_fd: RawFd,
        mailbox: Arc<SegQueue<WorkerMailboxMsg>>,
        mailboxes: Vec<Arc<SegQueue<WorkerMailboxMsg>>>,
        mailbox_fds: Vec<RawFd>,
        conn_id_alloc: Arc<AtomicU64>,
        recovery_coordinator: Arc<RecoveryCoordinator>,
        stop: Arc<AtomicBool>,
    ) -> RS<Self> {
        let worker_id = worker.worker_index();
        let ring = mudu_sys::uring::IoUring::new(1024);
        if let Err(rc) = ring {
            return Err(m_error!(
                EC::NetErr,
                format!("io_uring_queue_init_params error {}", rc)
            ));
        }
        let ring = ring.expect("checked above");
        let log = worker.worker_log().map(|backend| {
            new_xl_batch_worker_log(
                backend.clone(),
                recovery::WorkerRingLoopRecoveryHandler {
                    worker: worker.clone(),
                },
            )
        });
        let worker_local_ring = Arc::new(WorkerLocalRing::new());
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
            connection_task_fds: Arc::new(scc::HashMap::new()),
            callback_registry: CallbackRegistry::new(),
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

    /// Runs worker recovery and then enters the main service loop.
    ///
    /// The worker-local ring pointer is installed for the duration of the run
    /// so user-level async file I/O can enqueue requests onto this loop.
    pub(in crate::server) fn run(&mut self) -> RS<WorkerLoopStats> {
        scoped_task_trace!();
        trace!(
            worker_id = self.worker.worker_id(),
            "worker_ring_loop run start"
        );
        set_current_worker_local(as_worker_local_ref(new_session_bound_worker_runtime(
            self.worker.clone(),
            0,
        )));
        set_current_worker_ring(self.worker_local_ring.clone());
        set_current_message_bus(self.message_bus.as_ref());
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
        if let Err(err) = self.recover_worker_log() {
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

    #[allow(dead_code)]
    pub(in crate::server) fn spawn(&self, conn_id: Option<u64>, future: WorkerTaskFuture) {
        self.worker_local_ring
            .worker_task_registry()
            .spawn(conn_id, future);
    }

    pub(in crate::server) fn process_cqe(&mut self, cqe: mudu_sys::uring::Cqe) -> RS<()> {
        let token = cqe.user_data();
        let result = cqe.result();
        let op = self.inflight.remove(&token).ok_or_else(|| {
            m_error!(
                EC::InternalErr,
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
                    server_iouring::set_connection_options(conn_fd)?;
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
        let mut op = Box::new(AcceptOp::new(mudu_sys::uring::SockAddrBuf::new_empty()));
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
        let socket = crate::io::socket::IoSocket::from_raw_fd(fd);
        let _ = self.connection_task_fds.insert_sync(conn_id, fd);
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

    #[allow(dead_code)]
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

    #[allow(dead_code)]
    pub(in crate::server) fn cancel_async_callback(&mut self, callback_id: CallbackId) -> bool {
        self.callback_registry.cancel(callback_id)
    }

    #[allow(dead_code)]
    pub(in crate::server) fn fire_callback_event(&mut self, key: CallbackEventKey) -> RS<()> {
        let ready = self.callback_registry.fire_event(key);
        self.spawn_ready_callbacks(ready)
    }

    #[allow(dead_code)]
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

    #[allow(dead_code)]
    fn spawn_ready_callbacks(&mut self, callbacks: Vec<PendingCallback>) -> RS<()> {
        for pending in callbacks {
            let future = (pending.callback)();
            self.spawn(None, spawn_system_worker_task(future));
        }
        Ok(())
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use crate::io::file::{close, flush, open, read, write};
    use crate::io::socket::{
        accept, close as close_socket, connect, recv, send, shutdown, socket, IoSocket,
    };
    use crate::server::callback_registry::{CallbackDomain, CallbackEventKey, CallbackTrigger};
    use crate::server::worker_registry::load_or_create_worker_registry;
    use mudu::common::id::gen_oid;
    use mudu_sys::tokio::task::{yield_now, JoinHandle};
    use std::env::temp_dir;
    use std::io::{Read, Write};
    use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};

    fn test_worker_loop() -> WorkerRingLoop {
        let dir = temp_dir()
            .join(format!("worker_ring_loop_test_{}", gen_oid()))
            .to_string_lossy()
            .into_owned();
        let registry = load_or_create_worker_registry(&dir, 1).unwrap();
        let identity = registry.worker(0).cloned().unwrap();
        let worker = WorkerRuntime::new(
            identity,
            1,
            dir.clone(),
            dir.clone(),
            4096,
            None,
            registry,
            0,
        )
        .unwrap();
        let mailbox_fd = mudu_sys::sync_sync::eventfd().unwrap();
        WorkerRingLoop::new(
            worker,
            -1,
            mailbox_fd,
            Arc::new(SegQueue::new()),
            vec![Arc::new(SegQueue::new())],
            vec![mailbox_fd],
            Arc::new(AtomicU64::new(1)),
            Arc::new(RecoveryCoordinator::new(1, None)),
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap()
    }

    async fn drive_ring_future<T>(
        loop_state: &mut WorkerRingLoop,
        handle: &JoinHandle<RS<T>>,
    ) -> RS<()>
    where
        T: Send + 'static,
    {
        while !handle.is_finished() {
            loop_state.submit_user_ring_io_if_needed()?;
            let submitted = loop_state.ring.submit();
            if submitted < 0 {
                return Err(m_error!(
                    EC::NetErr,
                    format!("io_uring_submit error {}", submitted)
                ));
            }
            if loop_state.inflight.is_empty() {
                yield_now().await;
                continue;
            }
            let cqe = loop_state.ring.wait().map_err(|wait_rc| {
                m_error!(EC::NetErr, format!("io_uring_wait_cqe error {}", wait_rc))
            })?;
            loop_state.process_cqe(cqe)?;
            yield_now().await;
        }
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn worker_ring_loop_executes_user_file_io_via_cqe() {
        let mut loop_state = match std::panic::catch_unwind(test_worker_loop) {
            Ok(loop_state) => loop_state,
            Err(_) => return,
        };
        set_current_worker_ring(loop_state.worker_local_ring.clone());

        let path = temp_dir().join(format!("iouring_file_io_{}", gen_oid()));
        let path_str = path.to_string_lossy().into_owned();

        let open_task = mudu_sys::tokio::spawn({
            let path_str = path_str.clone();
            async move {
                open(
                    &path_str,
                    libc::O_CREAT | libc::O_RDWR | libc::O_TRUNC | libc::O_CLOEXEC,
                    0o644,
                )
                .await
            }
        });
        yield_now().await;
        drive_ring_future(&mut loop_state, &open_task)
            .await
            .unwrap();
        let file = open_task.await.unwrap().unwrap();

        let fd = file.fd();
        let write_task = mudu_sys::tokio::spawn(async move {
            write(
                &crate::io::file::IoFile::from_raw_fd(fd),
                b"hello iouring".to_vec(),
                0,
            )
            .await
        });
        yield_now().await;
        drive_ring_future(&mut loop_state, &write_task)
            .await
            .unwrap();
        assert_eq!(write_task.await.unwrap().unwrap(), b"hello iouring".len());

        let fd = file.fd();
        let flush_task = mudu_sys::tokio::spawn(async move {
            flush(&crate::io::file::IoFile::from_raw_fd(fd)).await
        });
        yield_now().await;
        drive_ring_future(&mut loop_state, &flush_task)
            .await
            .unwrap();
        flush_task.await.unwrap().unwrap();

        let fd = file.fd();
        let read_task = mudu_sys::tokio::spawn(async move {
            read(&crate::io::file::IoFile::from_raw_fd(fd), 13, 0).await
        });
        yield_now().await;
        drive_ring_future(&mut loop_state, &read_task)
            .await
            .unwrap();
        assert_eq!(read_task.await.unwrap().unwrap(), b"hello iouring".to_vec());

        assert_eq!(std::fs::read(&path).unwrap(), b"hello iouring".to_vec());

        let close_task = mudu_sys::tokio::spawn(async move { close(file).await });
        yield_now().await;
        drive_ring_future(&mut loop_state, &close_task)
            .await
            .unwrap();
        close_task.await.unwrap().unwrap();

        unset_current_worker_ring();
        loop_state.ring.exit();
        mudu_sys::sync_sync::close_fd(loop_state.mailbox_fd).unwrap();
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn worker_ring_loop_executes_user_socket_connect_io_via_cqe() {
        let mut loop_state = match std::panic::catch_unwind(test_worker_loop) {
            Ok(loop_state) => loop_state,
            Err(_) => return,
        };
        set_current_worker_ring(loop_state.worker_local_ring.clone());

        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let peer = thread::spawn(move || -> RS<()> {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0u8; 4];
            stream.read_exact(&mut buf).unwrap();
            assert_eq!(&buf, b"ping");
            stream.write_all(b"pong").unwrap();
            let mut eof = [0u8; 1];
            let read = stream.read(&mut eof).unwrap();
            assert_eq!(read, 0);
            Ok(())
        });

        let socket_task = mudu_sys::tokio::spawn(async {
            socket(libc::AF_INET, libc::SOCK_STREAM | libc::SOCK_CLOEXEC, 0).await
        });
        yield_now().await;
        drive_ring_future(&mut loop_state, &socket_task)
            .await
            .unwrap();
        let sock = socket_task.await.unwrap().unwrap();

        let fd = sock.fd();
        let connect_task =
            mudu_sys::tokio::spawn(async move { connect(&IoSocket::from_raw_fd(fd), addr).await });
        yield_now().await;
        drive_ring_future(&mut loop_state, &connect_task)
            .await
            .unwrap();
        connect_task.await.unwrap().unwrap();

        let fd = sock.fd();
        let send_task = mudu_sys::tokio::spawn(async move {
            send(&IoSocket::from_raw_fd(fd), b"ping".to_vec(), 0).await
        });
        yield_now().await;
        drive_ring_future(&mut loop_state, &send_task)
            .await
            .unwrap();
        assert_eq!(send_task.await.unwrap().unwrap(), 4);

        let fd = sock.fd();
        let recv_task =
            mudu_sys::tokio::spawn(async move { recv(&IoSocket::from_raw_fd(fd), 4, 0).await });
        yield_now().await;
        drive_ring_future(&mut loop_state, &recv_task)
            .await
            .unwrap();
        assert_eq!(recv_task.await.unwrap().unwrap(), b"pong".to_vec());

        let fd = sock.fd();
        let shutdown_task = mudu_sys::tokio::spawn(async move {
            shutdown(&IoSocket::from_raw_fd(fd), libc::SHUT_WR).await
        });
        yield_now().await;
        drive_ring_future(&mut loop_state, &shutdown_task)
            .await
            .unwrap();
        shutdown_task.await.unwrap().unwrap();

        let close_task = mudu_sys::tokio::spawn(async move { close_socket(sock).await });
        yield_now().await;
        drive_ring_future(&mut loop_state, &close_task)
            .await
            .unwrap();
        close_task.await.unwrap().unwrap();

        peer.join().unwrap().unwrap();

        unset_current_worker_ring();
        loop_state.ring.exit();
        mudu_sys::sync_sync::close_fd(loop_state.mailbox_fd).unwrap();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn worker_ring_loop_executes_user_socket_accept_io_via_cqe() {
        let mut loop_state = match std::panic::catch_unwind(test_worker_loop) {
            Ok(loop_state) => loop_state,
            Err(_) => return,
        };
        set_current_worker_ring(loop_state.worker_local_ring.clone());

        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let listener_fd = unsafe { libc::dup(listener.as_raw_fd()) };
        assert!(listener_fd >= 0);
        let listener_sock = IoSocket::from_raw_fd(listener_fd);

        let peer = thread::spawn(move || -> RS<()> {
            let mut stream = std::net::TcpStream::connect(addr).unwrap();
            stream.write_all(b"ping").unwrap();
            let mut buf = [0u8; 4];
            stream.read_exact(&mut buf).unwrap();
            assert_eq!(&buf, b"pong");
            Ok(())
        });

        let accept_fd = listener_sock.fd();
        let accept_task =
            mudu_sys::tokio::spawn(async move { accept(&IoSocket::from_raw_fd(accept_fd)).await });
        yield_now().await;
        drive_ring_future(&mut loop_state, &accept_task)
            .await
            .unwrap();
        let (accepted, remote_addr) = accept_task.await.unwrap().unwrap();
        assert_eq!(
            remote_addr.ip(),
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST)
        );

        let accepted_fd = accepted.fd();
        let recv_task =
            mudu_sys::tokio::spawn(
                async move { recv(&IoSocket::from_raw_fd(accepted_fd), 4, 0).await },
            );
        yield_now().await;
        drive_ring_future(&mut loop_state, &recv_task)
            .await
            .unwrap();
        assert_eq!(recv_task.await.unwrap().unwrap(), b"ping".to_vec());

        let accepted_fd = accepted.fd();
        let send_task = mudu_sys::tokio::spawn(async move {
            send(&IoSocket::from_raw_fd(accepted_fd), b"pong".to_vec(), 0).await
        });
        yield_now().await;
        drive_ring_future(&mut loop_state, &send_task)
            .await
            .unwrap();
        assert_eq!(send_task.await.unwrap().unwrap(), 4);

        let accepted_fd = accepted.fd();
        let shutdown_task = mudu_sys::tokio::spawn(async move {
            shutdown(&IoSocket::from_raw_fd(accepted_fd), libc::SHUT_WR).await
        });
        yield_now().await;
        drive_ring_future(&mut loop_state, &shutdown_task)
            .await
            .unwrap();
        shutdown_task.await.unwrap().unwrap();

        let close_accepted_task =
            mudu_sys::tokio::spawn(async move { close_socket(accepted).await });
        yield_now().await;
        drive_ring_future(&mut loop_state, &close_accepted_task)
            .await
            .unwrap();
        close_accepted_task.await.unwrap().unwrap();

        let close_listener_task =
            mudu_sys::tokio::spawn(async move { close_socket(listener_sock).await });
        yield_now().await;
        drive_ring_future(&mut loop_state, &close_listener_task)
            .await
            .unwrap();
        close_listener_task.await.unwrap().unwrap();

        peer.join().unwrap().unwrap();

        unset_current_worker_ring();
        loop_state.ring.exit();
        mudu_sys::sync_sync::close_fd(loop_state.mailbox_fd).unwrap();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn worker_ring_loop_runs_event_callback_as_system_task() {
        let mut loop_state = match std::panic::catch_unwind(test_worker_loop) {
            Ok(loop_state) => loop_state,
            Err(_) => return,
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
        mudu_sys::sync_sync::close_fd(loop_state.mailbox_fd).unwrap();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn worker_ring_loop_runs_sequence_callback_when_frontier_advances_and_skips_cancelled() {
        let mut loop_state = match std::panic::catch_unwind(test_worker_loop) {
            Ok(loop_state) => loop_state,
            Err(_) => return,
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
        mudu_sys::sync_sync::close_fd(loop_state.mailbox_fd).unwrap();
    }
}
