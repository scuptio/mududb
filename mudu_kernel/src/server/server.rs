use mudu_sys::async_rt::contract::AsyncRuntime;
use crate::server::async_func_runtime::AsyncFuncInvokerPtr;
use crate::server::async_func_task::HandleResult;

use crate::server::frame_dispatch::{dispatch_frame_async, try_decode_next_frame};
use crate::server::message_bus_api::{
    register_worker_message_bus, set_current_message_bus, unregister_worker_message_bus,
    unset_current_message_bus, EndpointId, Envelope, MessageBus, MessageBusRef, MessageId,
    OnRecvCallback, OutgoingMessage, RecvFilter, ServerInstanceId, SubscriptionId,
};
use crate::server::message_bus_state::WorkerMessageBusState;
use crate::server::session_bound_worker_runtime::{
    as_worker_local_ref, new_session_bound_worker_runtime,
};
use crate::server::worker::WorkerRuntime;
use crate::server::worker_local::{set_current_worker_local, unset_current_worker_local};
use crate::server::worker_registry::{WorkerIdentity, WorkerRegistry};
use crate::wal::worker_log::WorkerLogBatching;
use crate::wal::worker_log::{decode_frames, WorkerLogBackend};
use crate::wal::xl_batch::decode_xl_batches;
use async_trait::async_trait;
use crossbeam_queue::SegQueue;

use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use mudu_contract::protocol::encode_merror_response;
use mudu_sys::sync::stop_flag::{stop_channel, StopRx, StopTx};
use mudu_sys::tokio;
use mudu_sys::tokio::io::{AsyncReadExt, AsyncWriteExt};
use mudu_sys::tokio::net::{TcpListener as TokioTcpListener, TcpStream as TokioTcpStream};
use mudu_sys::tokio::sync::Notify;
use mudu_utils::notifier::{notify_wait, Notifier, Waiter};
use mudu_utils::scoped_task_trace;
use mudu_utils::task_async::{
    build_current_thread_runtime, spawn_local_detached, spawn_local_task, CurrentThreadTaskRuntime,
};

use socket2::{Domain, Protocol, Socket, Type};

use std::net::{SocketAddr, TcpListener};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{atomic::AtomicBool, Arc};
use mudu_sys::sync::SMutex;

use crate::server::server_launch::{ServerLaunch, WorkerTcpBackendConfig};
use std::thread;
use std::thread::JoinHandle;
use tracing::trace;

/// Backend entry point for the `client` transport.
///
/// Actual behavior is target-specific: Linux runs the native `io_uring`
/// backend, and other platforms run a semantically compatible fallback
/// implementation.
pub struct WorkerTcpBackend;
pub struct TokioTcpBackend;

/// Backward-compatible name for callers that still refer to the historical
/// io_uring-only backend entry point.
pub type IoUringTcpBackend = WorkerTcpBackend;

struct TokioWorkerMessageBus {
    local_worker_id: OID,
    registry: Arc<WorkerRegistry>,
    mailboxes: Vec<Arc<SegQueue<Envelope>>>,
    mailbox_wakes: Vec<Arc<Notify>>,
    next_msg_id: AtomicU64,
    state: SMutex<WorkerMessageBusState>,
}

impl TokioWorkerMessageBus {
    fn new(
        local_worker_id: OID,
        registry: Arc<WorkerRegistry>,
        mailboxes: Vec<Arc<SegQueue<Envelope>>>,
        mailbox_wakes: Vec<Arc<Notify>>,
    ) -> Arc<Self> {
        Arc::new(Self {
            local_worker_id,
            registry,
            mailboxes,
            mailbox_wakes,
            next_msg_id: AtomicU64::new(1),
            state: SMutex::new(WorkerMessageBusState::new()),
        })
    }

    fn bus_ref(self: &Arc<Self>) -> MessageBusRef {
        self.clone()
    }

    fn handle_incoming(&self, envelope: Envelope) -> RS<()> {
        let maybe_callback = {
            let mut state = self
                .state
                .lock()
                .map_err(|_| m_error!(EC::InternalErr, "tokio message bus state lock poisoned"))?;
            state.handle_incoming(envelope)
        };
        if let Some((callback, envelope)) = maybe_callback {
            let _ = spawn_local_detached("tokio_message_bus_handle_incoming", async move {
                let _ = (callback)(envelope).await;
            });
        }
        Ok(())
    }

    fn route_worker_index(&self, endpoint: EndpointId) -> RS<usize> {
        self.registry
            .worker_index_by_worker_id(endpoint)
            .ok_or_else(|| m_error!(EC::NoSuchElement, format!("no such worker id {}", endpoint)))
    }
}

#[async_trait]
impl MessageBus for TokioWorkerMessageBus {
    fn local_endpoint(&self) -> EndpointId {
        self.local_worker_id
    }

    async fn send(&self, dst: EndpointId, message: OutgoingMessage) -> RS<MessageId> {
        scoped_task_trace!();
        let msg_id = self.next_msg_id.fetch_add(1, Ordering::Relaxed);
        let envelope = Envelope::new(
            msg_id,
            message.correlation_id(),
            self.local_endpoint(),
            dst.clone(),
            message.kind(),
            message.payload_owned(),
            message.delivery(),
        );
        let target_worker = self.route_worker_index(dst)?;
        let Some(mailbox) = self.mailboxes.get(target_worker) else {
            return Err(m_error!(
                EC::InternalErr,
                format!("mailbox target worker {} is out of range", target_worker)
            ));
        };
        mailbox.push(envelope);
        if let Some(wake) = self.mailbox_wakes.get(target_worker) {
            wake.notify_one();
        }
        Ok(msg_id)
    }

    async fn recv(&self, filter: RecvFilter) -> RS<Envelope> {
        let receiver = {
            let mut state = self
                .state
                .lock()
                .map_err(|_| m_error!(EC::InternalErr, "tokio message bus state lock poisoned"))?;
            if let Some(envelope) = state.try_take_message(&filter) {
                return Ok(envelope);
            }
            state.register_waiter(filter)
        };
        receiver
            .wait()
            .await?
            .ok_or_else(|| m_error!(EC::ThreadErr, "message bus waiter dropped before delivery"))
    }

    fn on_recv_callback(&self, filter: RecvFilter, callback: OnRecvCallback) -> RS<SubscriptionId> {
        let (callback_id, maybe_envelope) = {
            let mut state = self
                .state
                .lock()
                .map_err(|_| m_error!(EC::InternalErr, "tokio message bus state lock poisoned"))?;
            state.register_callback(filter, callback.clone())
        };
        if let Some(envelope) = maybe_envelope {
            let _ = spawn_local_detached("tokio_message_bus_on_recv_callback", async move {
                let _ = (callback)(envelope).await;
            });
        }
        Ok(callback_id)
    }

    fn cancel_callback(&self, id: SubscriptionId) -> RS<bool> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| m_error!(EC::InternalErr, "tokio message bus state lock poisoned"))?;
        Ok(state.cancel_callback(id))
    }
}

unsafe impl Send for TokioWorkerMessageBus {}
unsafe impl Sync for TokioWorkerMessageBus {}

struct WorkerBuildConfig {
    server_instance_id: ServerInstanceId,
    worker_count: usize,
    log_dir: String,
    data_dir: String,
    log_chunk_size: u64,
    log_batching: WorkerLogBatching,
    procedure_runtime: Option<AsyncFuncInvokerPtr>,
    worker_identity: WorkerIdentity,
    worker_registry: Arc<WorkerRegistry>,
    async_runtime: Option<Arc<dyn AsyncRuntime>>,
}

impl WorkerBuildConfig {
    fn from_server_config(cfg: &WorkerTcpBackendConfig, worker_id: usize) -> RS<Self> {
        let server_cfg = cfg.cfg();
        let deps = cfg.deps();
        let worker_identity = deps
            .worker_registry()
            .worker(worker_id)
            .cloned()
            .ok_or_else(|| {
                m_error!(
                    EC::NoSuchElement,
                    format!("missing worker identity {}", worker_id)
                )
            })?;
        Ok(Self {
            server_instance_id: server_cfg.server_instance_id(),
            worker_count: server_cfg.worker_count(),
            log_dir: server_cfg.log_dir().to_string(),
            data_dir: server_cfg.data_dir().to_string(),
            log_chunk_size: server_cfg.log_chunk_size(),
            log_batching: deps.log_batching(),
            procedure_runtime: deps.procedure_runtime_for_worker(worker_id),
            worker_identity,
            worker_registry: deps.worker_registry(),
            async_runtime: deps.async_runtime(),
        })
    }

    async fn build_worker(self) -> RS<WorkerRuntime> {
        WorkerRuntime::new_with_log_batching_and_runtime(
            self.worker_identity,
            self.worker_count,
            self.log_dir,
            self.data_dir,
            self.log_chunk_size,
            self.log_batching,
            self.procedure_runtime,
            self.worker_registry,
            self.async_runtime,
            self.server_instance_id,
        ).await
    }
}

fn spawn_stop_bridge(
    name: &'static str,
    stop: Waiter,
    stop_flag: Arc<AtomicBool>,
    service_ready: Arc<AtomicBool>,
    stop_tx: StopTx,
) -> RS<JoinHandle<RS<()>>> {
    thread::Builder::new()
        .name(name.to_string())
        .spawn(move || {
            let runtime = build_current_thread_runtime().map_err(|e| {
                m_error!(EC::TokioErr, format!("create runtime for {name} error"), e)
            })?;
            trace!(bridge = name, "tokio stop bridge waiting for stop");
            runtime.block_on(stop.wait());
            trace!(bridge = name, "tokio stop bridge observed stop");
            service_ready.store(false, Ordering::Relaxed);
            stop_flag.store(true, Ordering::Relaxed);
            stop_tx.stop();
            Ok(())
        })
        .map_err(|e| m_error!(EC::ThreadErr, format!("spawn {name} error"), e))
}

fn wait_stop_bridge(name: &'static str, handle: JoinHandle<RS<()>>) -> RS<()> {
    handle
        .join()
        .map_err(|_| m_error!(EC::ThreadErr, format!("join {name} error")))?
}

impl WorkerTcpBackend {
    /// Starts the backend until shutdown.
    ///
    /// This method keeps the old public entry point stable. It dispatches to
    /// the io_uring implementation on Linux. Select `TokioTcpBackend`
    /// explicitly when the Tokio worker loop is desired on any target.
    pub fn sync_serve(cfg: ServerLaunch) -> RS<()> {
        let (_stop_notifier, stop_waiter) = notify_wait();
        Self::sync_serve_with_stop(cfg, stop_waiter)
    }

    /// Internal serve entry that accepts an explicit stop waiter.
    ///
    /// The io_uring backend is Linux-only. The Tokio backend is available as a
    /// separate implementation and bridges the async stop signal into its
    /// worker loop.
    pub fn sync_serve_with_stop(cfg: ServerLaunch, stop: Waiter) -> RS<()> {
        Self::sync_serve_with_stop_and_ready(cfg, stop, None)
    }

    pub fn sync_serve_with_stop_and_ready(
        cfg: ServerLaunch,
        stop: Waiter,
        ready: Option<Notifier>,
    ) -> RS<()> {
        #[cfg(target_os = "linux")]
        {
            return crate::server::server_iouring::sync_serve_iouring(cfg, stop, ready);
        }

        #[cfg(not(target_os = "linux"))]
        TokioTcpBackend::sync_serve_with_stop_and_ready(cfg, stop, ready)
    }
}

impl TokioTcpBackend {
    pub fn sync_serve(cfg: ServerLaunch) -> RS<()> {
        let (_stop_notifier, stop_waiter) = notify_wait();
        Self::sync_serve_with_stop(cfg, stop_waiter)
    }

    pub fn sync_serve_with_stop(cfg: ServerLaunch, stop: Waiter) -> RS<()> {
        Self::sync_serve_with_stop_and_ready(cfg, stop, None)
    }

    pub fn sync_serve_with_stop_and_ready(
        cfg: ServerLaunch,
        stop: Waiter,
        ready: Option<Notifier>,
    ) -> RS<()> {
        let stop_flag = Arc::new(AtomicBool::new(false));
        let service_ready = Arc::new(AtomicBool::new(false));
        let (stop_tx, stop_rx) = stop_channel();
        let notifier = spawn_stop_bridge(
            "tokio-stop-bridge",
            stop,
            stop_flag.clone(),
            service_ready.clone(),
            stop_tx,
        )?;
        let result = sync_serve_tokio(cfg, stop_flag, stop_rx, service_ready, ready);
        wait_stop_bridge("tokio-stop-bridge", notifier)?;
        result
    }
}

#[derive(Clone)]
struct TokioConnTaskState {
    active: Arc<std::sync::atomic::AtomicU64>,
    drained: Arc<Notify>,
}

impl TokioConnTaskState {
    fn new() -> Self {
        Self {
            active: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            drained: Arc::new(Notify::new()),
        }
    }

    fn on_spawn(&self) {
        self.active.fetch_add(1, Ordering::Relaxed);
    }

    fn on_finish(&self) {
        if self.active.fetch_sub(1, Ordering::Relaxed) == 1 {
            self.drained.notify_waiters();
        }
    }

    async fn wait_drained(&self) {
        while self.active.load(Ordering::Relaxed) > 0 {
            self.drained.notified().await;
        }
    }
}

fn sync_serve_tokio(
    mut cfg: ServerLaunch,
    stop: Arc<AtomicBool>,
    stop_rx: StopRx,
    service_ready: Arc<AtomicBool>,
    ready: Option<Notifier>,
) -> RS<()> {
    if cfg.cfg().worker_count() == 0 {
        return Err(m_error!(EC::ParseErr, "invalid tokio worker count"));
    }
    let conn_id_alloc = Arc::new(AtomicU64::new(1));
    let bus_mailboxes: Vec<_> = (0..cfg.cfg().worker_count())
        .map(|_| Arc::new(SegQueue::<Envelope>::new()))
        .collect();
    let bus_wakes: Vec<_> = (0..cfg.cfg().worker_count())
        .map(|_| Arc::new(Notify::new()))
        .collect();
    let (started_tx, started_rx) = mpsc::channel::<RS<()>>();
    let (rpc_ready_tx, rpc_ready_rx) = mpsc::channel::<RS<()>>();

    let mut handles = Vec::with_capacity(cfg.cfg().worker_count());
    for worker_id in 0..cfg.cfg().worker_count() {
        let worker_cfg = WorkerBuildConfig::from_server_config(&cfg, worker_id)?;
        let bus_inbox = bus_mailboxes[worker_id].clone();
        let bus_wake = bus_wakes[worker_id].clone();
        let all_bus_mailboxes = bus_mailboxes.clone();
        let all_bus_wakes = bus_wakes.clone();
        let conn_id_alloc = conn_id_alloc.clone();
        let stop = stop.clone();
        let stop_rx = stop_rx.clone();
        let service_ready = service_ready.clone();
        let started_tx = started_tx.clone();
        let rpc_ready_tx = rpc_ready_tx.clone();
        let listener = if let Some(prebound) = cfg.take_prebound_listener() {
            prebound
        } else {
            let worker_port = cfg.cfg().listen_port_for_worker(worker_id)?;
            let listen_addr: SocketAddr = format!("{}:{}", cfg.cfg().listen_ip(), worker_port)
                .parse()
                .map_err(|e| {
                    m_error!(
                        EC::ParseErr,
                        format!("parse tokio tcp listen address error: {}", worker_port),
                        e
                    )
                })?;
            create_listener(listen_addr)?
        };
        let handle = thread::Builder::new()
            .name(format!("tokio-tcp-worker-{worker_id}"))
            .spawn(move || {
                let runtime = CurrentThreadTaskRuntime::new()
                    .map_err(|e| m_error!(EC::TokioErr, "build tokio worker runtime error", e))?;
                let result = runtime.block_on(async move {
                    trace!(worker_id, "tokio worker thread starting");
                    listener
                        .set_nonblocking(true)
                        .map_err(|e| m_error!(EC::NetErr, "set tokio listener nonblocking error", e))?;
                    let worker = worker_cfg.build_worker().await?;
                    worker.bootstrap_storage_async().await.map_err(|e| {
                        m_error!(EC::StorageErr, "bootstrap worker storage failed", e)
                    })?;
                    let message_bus = TokioWorkerMessageBus::new(
                        worker.worker_id(),
                        worker.registry().clone(),
                        all_bus_mailboxes,
                        all_bus_wakes,
                    );
                    let worker_id = worker.worker_id();
                    let server_instance_id = worker.server_instance_id();
                    let conn_tasks = TokioConnTaskState::new();
                    set_current_worker_local(as_worker_local_ref(new_session_bound_worker_runtime(
                        worker.clone(),
                        0,
                    )));
                    let message_bus_ref = message_bus.bus_ref();
                    set_current_message_bus(message_bus_ref.clone());
                    register_worker_message_bus(
                        server_instance_id,
                        worker.worker_id(),
                        &message_bus_ref,
                    )?;
                    trace!(worker_id, "tokio worker loop entering");
                    let listener = TokioTcpListener::from_std(listener)
                        .map_err(|e| m_error!(EC::NetErr, "convert tokio tcp listener error", e))?;
                    worker.ensure_partition_rpc_handler()?;
                    recover_worker_log_tokio(&worker).await?;
                    let (_task_notifier, task_waiter) = notify_wait();
                    let join = spawn_local_task(
                        task_waiter.into(),
                        &format!("tokio_worker_loop_{worker_id}"),
                        run_worker_loop_tokio(
                            worker,
                            listener,
                            bus_inbox,
                            message_bus,
                            bus_wake,
                            conn_id_alloc,
                            stop,
                            stop_rx,
                            service_ready,
                            conn_tasks.clone(),
                            Some(rpc_ready_tx),
                        ),
                    )?;
                    let _ = started_tx.send(Ok(()));
                    let _ = unregister_worker_message_bus(server_instance_id, worker_id);
                    unset_current_message_bus();
                    unset_current_worker_local();
                    match join.await.map_err(|e| {
                        m_error!(EC::TokioErr, "join tokio worker loop task error", e)
                    })? {
                        Some(result) => result,
                        None => Ok(()),
                    }
                });
                trace!(worker_id, ok = result.is_ok(), "tokio worker loop returned");

                trace!(worker_id, "tokio worker thread exiting");
                result
            })
            .map_err(|e| m_error!(EC::ThreadErr, "spawn tokio worker error", e))?;
        handles.push(handle);
    }
    drop(started_tx);
    drop(rpc_ready_tx);

    for _ in 0..cfg.cfg().worker_count() {
        let started = started_rx.recv().map_err(|_| {
            m_error!(
                EC::ThreadErr,
                "tokio worker startup barrier channel closed unexpectedly"
            )
        })?;
        started?;
    }

    // RPC-ready barrier: every worker must report that its message bus,
    // partition rpc handler and main loop are active before the backend is
    // externally considered ready.
    for _ in 0..cfg.cfg().worker_count() {
        let ready = rpc_ready_rx.recv().map_err(|_| {
            m_error!(
                EC::ThreadErr,
                "tokio worker rpc-ready barrier channel closed unexpectedly"
            )
        })?;
        ready?;
    }
    service_ready.store(true, Ordering::Relaxed);

    // Tokio mode has no separate recovery barrier after the listener is bound
    // and the worker threads are spawned, so this is the earliest point where
    // callers can treat the backend as logically ready to serve requests.
    if let Some(ready) = ready {
        ready.notify_all();
    }

    for (worker_id, handle) in handles.into_iter().enumerate() {
        trace!(worker_id, "joining tokio worker");
        let result = handle
            .join()
            .map_err(|_| m_error!(EC::ThreadErr, "join tokio worker error"))?;
        trace!(worker_id, ok = result.is_ok(), "joined tokio worker");
        result?;
    }
    Ok(())
}

async fn run_worker_loop_tokio(
    worker: WorkerRuntime,
    listener: TokioTcpListener,
    bus_inbox: Arc<SegQueue<Envelope>>,
    message_bus: Arc<TokioWorkerMessageBus>,
    bus_wake: Arc<Notify>,
    conn_id_alloc: Arc<AtomicU64>,
    stop: Arc<AtomicBool>,
    mut stop_rx: StopRx,
    service_ready: Arc<AtomicBool>,
    conn_tasks: TokioConnTaskState,
    rpc_ready_tx: Option<mpsc::Sender<RS<()>>>,
) -> RS<()> {
    scoped_task_trace!();
    if let Some(tx) = rpc_ready_tx {
        let _ = tx.send(Ok(()));
    }
    while !stop.load(Ordering::Relaxed) {
        if stop_rx.is_stopped() {
            break;
        }
        while drain_message_bus_tokio(bus_inbox.as_ref(), message_bus.as_ref())? {}
        tokio::select! {
            accept_result = listener.accept() => {
                let (stream, remote_addr) = accept_result
                    .map_err(|err| m_error!(EC::NetErr, "accept tokio tcp connection error", err))?;
                let conn_id = conn_id_alloc.fetch_add(1, Ordering::Relaxed);
                let worker = worker.clone();
                let stop = stop.clone();
                let service_ready = service_ready.clone();
                let conn_tasks = conn_tasks.clone();
                trace!(
                    worker_id = worker.worker_id(),
                    conn_id,
                    remote = %remote_addr,
                    "tokio accepted connection"
                );
                conn_tasks.on_spawn();
                let stop_rx_conn = stop_rx.clone();
                let _ = spawn_local_detached(
                    &format!("tokio_conn_{conn_id}"),
                    async move {
                        let result =
                            handle_tokio_connection(
                                worker,
                                stream,
                                conn_id,
                                remote_addr,
                                stop,
                                stop_rx_conn,
                                service_ready,
                            )
                                .await;
                        conn_tasks.on_finish();
                        result
                    },
                );
            }
            _ = bus_wake.notified() => {}
            changed = stop_rx.changed() => {
                if !changed || stop_rx.is_stopped() {
                    break;
                }
            }
            else => {
                break;
            }
        }
    }
    let _ =
        tokio::time::timeout(std::time::Duration::from_secs(3), conn_tasks.wait_drained()).await;
    trace!(
        worker_id = worker.worker_id(),
        "tokio worker loop observed stop"
    );
    Ok(())
}

async fn recover_worker_log_tokio(worker: &WorkerRuntime) -> RS<()> {
    let Some(log) = worker.worker_log() else {
        return Ok(());
    };
    let chunk_paths = log.chunk_paths_sorted().await?;
    for path in chunk_paths {
        let bytes = std::fs::read(&path).map_err(|e| {
            m_error!(
                EC::IOErr,
                format!("read worker log chunk {} error", path.display()),
                e
            )
        })?;
        if bytes.is_empty() {
            continue;
        }
        let frames = decode_frames(&bytes)?;
        let batches = decode_xl_batches(&frames)?;
        for batch in batches {
            worker.replay_log_batch(batch).await?;
        }
    }
    Ok(())
}

async fn handle_tokio_connection(
    worker: WorkerRuntime,
    mut stream: TokioTcpStream,
    conn_id: u64,
    remote_addr: SocketAddr,
    stop: Arc<AtomicBool>,
    mut stop_rx: StopRx,
    service_ready: Arc<AtomicBool>,
) -> RS<()> {
    scoped_task_trace!();
    stream
        .set_nodelay(true)
        .map_err(|e| m_error!(EC::NetErr, "set tokio connection nodelay error", e))?;
    let mut read_buf: Vec<u8> = Vec::with_capacity(8192);
    let mut chunk = vec![0u8; 8192];
    loop {
        if stop.load(Ordering::Relaxed) || stop_rx.is_stopped() {
            break;
        }
        let read = tokio::select! {
            read_result = stream.read(&mut chunk) => {
                read_result.map_err(|e| m_error!(EC::NetErr, "read tokio tcp request error", e))?
            }
            changed = stop_rx.changed() => {
                if !changed || stop_rx.is_stopped() {
                    break;
                }
                continue;
            }
        };
        if read == 0 {
            break;
        }
        read_buf.extend_from_slice(&chunk[..read]);
        while let Some((frame, consumed)) = try_decode_next_frame(&read_buf)? {
            read_buf.drain(0..consumed);
            if !service_ready.load(Ordering::Relaxed) {
                let err = m_error!(EC::InternalErr, "server is not ready");
                let payload = encode_merror_response(frame.header().request_id(), &err)?;
                stream
                    .write_all(&payload)
                    .await
                    .map_err(|e| m_error!(EC::NetErr, "write tokio tcp response error", e))?;
                continue;
            }
            match dispatch_frame_async(&worker, conn_id, &frame).await {
                Ok(HandleResult::Response(payload)) => {
                    stream
                        .write_all(&payload)
                        .await
                        .map_err(|e| m_error!(EC::NetErr, "write tokio tcp response error", e))?;
                }
                Err(err) => {
                    let payload = encode_merror_response(frame.header().request_id(), &err)?;
                    stream
                        .write_all(&payload)
                        .await
                        .map_err(|e| m_error!(EC::NetErr, "write tokio tcp response error", e))?;
                }
            }
        }
    }
    worker.close_connection_sessions(conn_id)?;
    trace!(worker_id = worker.worker_id(), conn_id, remote = %remote_addr, "tokio connection closed");
    Ok(())
}

fn drain_message_bus_tokio(
    inbox: &SegQueue<Envelope>,
    message_bus: &TokioWorkerMessageBus,
) -> RS<bool> {
    let mut progressed = false;
    while let Some(envelope) = inbox.pop() {
        progressed = true;
        message_bus.handle_incoming(envelope)?;
    }
    Ok(progressed)
}

fn create_listener(listen_addr: SocketAddr) -> RS<TcpListener> {
    let domain = if listen_addr.is_ipv4() {
        Domain::IPV4
    } else {
        Domain::IPV6
    };
    let socket = Socket::new(domain, Type::STREAM, Some(Protocol::TCP))
        .map_err(|e| m_error!(EC::NetErr, "create tcp listener socket error", e))?;
    socket
        .set_reuse_address(true)
        .map_err(|e| m_error!(EC::NetErr, "enable SO_REUSEADDR error", e))?;
    socket
        .set_nonblocking(true)
        .map_err(|e| m_error!(EC::NetErr, "set listener nonblocking error", e))?;
    socket
        .bind(&listen_addr.into())
        .map_err(|e| m_error!(EC::NetErr, "bind io_uring tcp listener error", e))?;
    socket
        .listen(1024)
        .map_err(|e| m_error!(EC::NetErr, "listen io_uring tcp listener error", e))?;
    Ok(socket.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use mudu_contract::protocol::encode_get_request;
    use mudu_contract::protocol::GetRequest;
    use mudu_contract::protocol::HEADER_LEN;

    #[test]
    fn try_decode_next_frame_waits_for_full_payload() {
        let encoded = encode_get_request(1, &GetRequest::new(1, b"k".to_vec())).unwrap();
        assert!(try_decode_next_frame(&encoded[..HEADER_LEN - 1])
            .unwrap()
            .is_none());
        assert!(try_decode_next_frame(&encoded[..HEADER_LEN])
            .unwrap()
            .is_none());
        let decoded = try_decode_next_frame(&encoded).unwrap().unwrap();
        assert_eq!(decoded.0.header().request_id(), 1);
        assert_eq!(decoded.1, encoded.len());
    }
}
