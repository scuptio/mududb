#![allow(dead_code)]

use crate::async_rt::contract::AsyncRuntime;
use crate::server::async_func_runtime::AsyncFuncInvokerPtr;
use crate::server::async_func_task::{AsyncFuncFuture, AsyncFuncTask, HandleResult};
use crate::server::async_func_task_waker::AsyncFuncTaskWaker;
use crate::server::frame_dispatch::{dispatch_frame_async, try_decode_next_frame};
use crate::server::message_bus_api::{
    register_worker_message_bus, set_current_message_bus, unregister_worker_message_bus,
    unset_current_message_bus, EndpointId, Envelope, MessageBus, MessageBusRef, MessageId,
    OnRecvCallback, OutgoingMessage, RecvFilter, ServerInstanceId, SubscriptionId,
};
use crate::server::message_bus_state::WorkerMessageBusState;
use crate::server::routing::{ConnectionTransfer, RoutingMode, SessionOpenTransferAction};
use crate::server::session_bound_worker_runtime::{
    as_worker_local_ref, new_session_bound_worker_runtime,
};
use crate::server::worker::WorkerRuntime;
use crate::server::worker_local::{set_current_worker_local, unset_current_worker_local};
use crate::server::worker_registry::{
    load_or_create_worker_registry, WorkerIdentity, WorkerRegistry,
};
use crate::wal::worker_log::WorkerLogBatching;
use async_trait::async_trait;
use crossbeam_queue::SegQueue;
use futures::future::poll_fn;
use futures::task::{waker, Context};
use futures::Future;
use mudu::common::id::{gen_oid, OID};
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use mudu_contract::protocol::{
    encode_merror_response, encode_session_create_response, Frame, SessionCreateResponse,
};
use mudu_sys::tokio::net::{TcpListener as TokioTcpListener, TcpStream as TokioTcpStream};
use mudu_utils::notifier::{notify_wait, Notifier, Waiter};
use mudu_utils::task_async::{
    build_current_thread_runtime, spawn_local_detached, spawn_local_task, CurrentThreadTaskRuntime,
    PollTaskIdGuard,
};
use mudu_utils::task_context::TaskContext;
use mudu_utils::task_id::new_task_id;
use socket2::{Domain, Protocol, Socket, Type};
use std::collections::HashMap;
use std::io::ErrorKind;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{atomic::AtomicBool, Arc, Mutex};
use std::task::Poll;
use std::thread;
use std::thread::JoinHandle;
use std::time::Duration;
use tracing::trace;

/// Configuration shared by both execution paths of the `client` backend.
///
/// The same configuration is consumed by both the io_uring worker-ring backend
/// and the Tokio backend so they keep the worker model and protocol surface
/// aligned.
pub struct WorkerTcpServerConfig {
    server_instance_id: ServerInstanceId,
    worker_count: usize,
    listen_ip: String,
    listen_port: u16,
    prebound_listener: Option<TcpListener>,
    data_dir: String,
    log_dir: String,
    log_chunk_size: u64,
    log_batching: WorkerLogBatching,
    routing_mode: RoutingMode,
    procedure_runtime: Option<AsyncFuncInvokerPtr>,
    worker_procedure_runtimes: Option<Vec<AsyncFuncInvokerPtr>>,
    worker_registry: Arc<WorkerRegistry>,
    async_runtime: Option<Arc<dyn AsyncRuntime>>,
}

/// Backward-compatible name for callers that still refer to the historical
/// io_uring-only server configuration.
pub type IoUringTcpServerConfig = WorkerTcpServerConfig;

/// Alias used by backend construction code that does not need a transport-
/// specific name.
pub type WorkerTcpBackendConfig = WorkerTcpServerConfig;

impl WorkerTcpServerConfig {
    /// Creates a backend configuration.
    ///
    /// The resulting value can be used by both the io_uring and Tokio TCP
    /// backends with the same externally visible behavior.
    pub fn new(
        worker_count: usize,
        listen_ip: String,
        listen_port: u16,
        data_dir: String,
        log_dir: String,
        routing_mode: RoutingMode,
        procedure_runtime: Option<AsyncFuncInvokerPtr>,
    ) -> RS<Self> {
        let worker_registry = load_or_create_worker_registry(&log_dir, worker_count)?;
        Ok(Self {
            server_instance_id: gen_oid(),
            worker_count,
            listen_ip,
            listen_port,
            prebound_listener: None,
            data_dir,
            log_dir,
            log_chunk_size: 64 * 1024 * 1024,
            log_batching: WorkerLogBatching::default(),
            routing_mode,
            procedure_runtime,
            worker_procedure_runtimes: None,
            worker_registry,
            async_runtime: None,
        })
    }

    pub fn with_log_chunk_size(mut self, log_chunk_size: u64) -> Self {
        self.log_chunk_size = log_chunk_size;
        self
    }

    pub fn with_log_batching(mut self, log_batching: WorkerLogBatching) -> Self {
        self.log_batching = log_batching;
        self
    }

    pub fn with_prebound_listener(mut self, listener: TcpListener) -> Self {
        self.prebound_listener = Some(listener);
        self
    }

    pub fn with_worker_registry(mut self, worker_registry: Arc<WorkerRegistry>) -> RS<Self> {
        if worker_registry.workers().len() != self.worker_count {
            return Err(m_error!(
                EC::ParseErr,
                format!(
                    "worker registry count {} does not match expected {}",
                    worker_registry.workers().len(),
                    self.worker_count
                )
            ));
        }
        self.worker_registry = worker_registry;
        Ok(self)
    }

    /// Installs per-worker procedure runtimes.
    ///
    /// When this is not set, every worker uses `procedure_runtime()`. This hook
    /// exists so upper layers can give each worker an isolated invoker instance
    /// while keeping the transport API unchanged across io_uring and Tokio
    /// implementations.
    pub fn with_worker_procedure_runtimes(mut self, runtimes: Vec<AsyncFuncInvokerPtr>) -> Self {
        self.worker_procedure_runtimes = Some(runtimes);
        self
    }

    pub fn with_async_runtime(mut self, async_runtime: Arc<dyn AsyncRuntime>) -> Self {
        self.async_runtime = Some(async_runtime);
        self
    }

    pub fn server_instance_id(&self) -> ServerInstanceId {
        self.server_instance_id
    }

    pub fn worker_count(&self) -> usize {
        self.worker_count
    }

    pub fn listen_ip(&self) -> &str {
        &self.listen_ip
    }

    pub fn listen_port(&self) -> u16 {
        self.listen_port
    }

    pub fn take_prebound_listener(&mut self) -> Option<TcpListener> {
        self.prebound_listener.take()
    }

    pub fn log_dir(&self) -> &str {
        &self.log_dir
    }

    pub fn data_dir(&self) -> &str {
        &self.data_dir
    }

    pub fn log_chunk_size(&self) -> u64 {
        self.log_chunk_size
    }

    pub fn log_batching(&self) -> WorkerLogBatching {
        self.log_batching
    }

    pub fn routing_mode(&self) -> RoutingMode {
        self.routing_mode
    }

    pub fn worker_registry(&self) -> Arc<WorkerRegistry> {
        self.worker_registry.clone()
    }

    pub fn procedure_runtime(&self) -> Option<AsyncFuncInvokerPtr> {
        self.procedure_runtime.clone()
    }

    pub fn procedure_runtime_for_worker(&self, worker_id: usize) -> Option<AsyncFuncInvokerPtr> {
        self.worker_procedure_runtimes
            .as_ref()
            .and_then(|runtimes| runtimes.get(worker_id).cloned())
            .or_else(|| self.procedure_runtime())
    }

    pub fn async_runtime(&self) -> Option<Arc<dyn AsyncRuntime>> {
        self.async_runtime.clone()
    }
}

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

#[derive(Debug)]
struct TransferredConnection {
    transfer: ConnectionTransfer,
    stream: TcpStream,
    session_ids: Vec<OID>,
    session_open_action: Option<SessionOpenTransferAction>,
}

struct TokioWorkerConnection {
    core: ConnectionCore,
    stream: Option<TokioTcpStream>,
}

struct ConnectionCore {
    conn_id: u64,
    state: crate::server::connection_state::ConnectionState,
    remote_addr: SocketAddr,
    transferred: bool,
    read_buf: Vec<u8>,
    write_buf: Vec<u8>,
}

impl ConnectionCore {
    fn new(conn_id: u64, remote_addr: SocketAddr) -> Self {
        Self {
            conn_id,
            state: crate::server::connection_state::ConnectionState::Active,
            remote_addr,
            transferred: false,
            read_buf: Vec::with_capacity(4096),
            write_buf: Vec::with_capacity(4096),
        }
    }
}

trait BackendConnection {
    fn core(&self) -> &ConnectionCore;
    fn core_mut(&mut self) -> &mut ConnectionCore;
    fn read_available(&mut self) -> RS<bool>;
    fn write_pending(&mut self) -> RS<bool>;
    fn take_transfer_stream(&mut self) -> RS<TcpStream>;
}

impl BackendConnection for TokioWorkerConnection {
    fn core(&self) -> &ConnectionCore {
        &self.core
    }

    fn core_mut(&mut self) -> &mut ConnectionCore {
        &mut self.core
    }

    fn read_available(&mut self) -> RS<bool> {
        let mut progressed = false;
        let Some(stream) = self.stream.as_mut() else {
            return Ok(progressed);
        };
        let mut buf = [0u8; 8192];
        loop {
            match stream.try_read(&mut buf) {
                Ok(0) => {
                    self.core.state = crate::server::connection_state::ConnectionState::Closing;
                    break;
                }
                Ok(read) => {
                    progressed = true;
                    self.core.read_buf.extend_from_slice(&buf[..read]);
                }
                Err(err) if err.kind() == ErrorKind::WouldBlock => break,
                Err(err) => return Err(m_error!(EC::NetErr, "read tokio tcp request error", err)),
            }
        }
        Ok(progressed)
    }

    fn write_pending(&mut self) -> RS<bool> {
        let mut progressed = false;
        let Some(stream) = self.stream.as_mut() else {
            return Ok(progressed);
        };
        while !self.core.write_buf.is_empty() {
            match stream.try_write(&self.core.write_buf) {
                Ok(0) => {
                    self.core.state = crate::server::connection_state::ConnectionState::Closing;
                    break;
                }
                Ok(written) => {
                    progressed = true;
                    self.core.write_buf.drain(0..written);
                }
                Err(err) if err.kind() == ErrorKind::WouldBlock => break,
                Err(err) => {
                    return Err(m_error!(EC::NetErr, "write tokio tcp response error", err))
                }
            }
        }
        Ok(progressed)
    }

    fn take_transfer_stream(&mut self) -> RS<TcpStream> {
        let stream = self
            .stream
            .take()
            .ok_or_else(|| m_error!(EC::InternalErr, "tokio connection stream missing"))?;
        stream
            .into_std()
            .map_err(|e| m_error!(EC::NetErr, "convert tokio stream for transfer error", e))
    }
}

struct TokioWorkerMessageBus {
    local_worker_id: OID,
    registry: Arc<WorkerRegistry>,
    mailboxes: Vec<Arc<SegQueue<Envelope>>>,
    next_msg_id: AtomicU64,
    state: Mutex<WorkerMessageBusState>,
}

impl TokioWorkerMessageBus {
    fn new(
        local_worker_id: OID,
        registry: Arc<WorkerRegistry>,
        mailboxes: Vec<Arc<SegQueue<Envelope>>>,
    ) -> Arc<Self> {
        Arc::new(Self {
            local_worker_id,
            registry,
            mailboxes,
            next_msg_id: AtomicU64::new(1),
            state: Mutex::new(WorkerMessageBusState::new()),
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

    fn route_worker_index(&self, endpoint: &EndpointId) -> RS<usize> {
        match endpoint {
            EndpointId::Worker(worker_id) => self
                .registry
                .worker_index_by_worker_id(*worker_id)
                .ok_or_else(|| {
                    m_error!(
                        EC::NoSuchElement,
                        format!("no such worker id {}", worker_id)
                    )
                }),
            EndpointId::External(external_id) => Err(m_error!(
                EC::NotImplemented,
                format!("external endpoint {} is not implemented yet", external_id)
            )),
            EndpointId::Session(session_id) => Err(m_error!(
                EC::NotImplemented,
                format!("session endpoint {} is not implemented yet", session_id)
            )),
        }
    }
}

#[async_trait]
impl MessageBus for TokioWorkerMessageBus {
    fn local_endpoint(&self) -> EndpointId {
        EndpointId::Worker(self.local_worker_id)
    }

    async fn send(&self, dst: EndpointId, message: OutgoingMessage) -> RS<MessageId> {
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
        let target_worker = self.route_worker_index(&dst)?;
        let Some(mailbox) = self.mailboxes.get(target_worker) else {
            return Err(m_error!(
                EC::InternalErr,
                format!("mailbox target worker {} is out of range", target_worker)
            ));
        };
        mailbox.push(envelope);
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
    routing_mode: RoutingMode,
    procedure_runtime: Option<AsyncFuncInvokerPtr>,
    worker_identity: WorkerIdentity,
    worker_registry: Arc<WorkerRegistry>,
    async_runtime: Option<Arc<dyn AsyncRuntime>>,
}

impl WorkerBuildConfig {
    fn from_server_config(cfg: &WorkerTcpBackendConfig, worker_id: usize) -> RS<Self> {
        let worker_identity = cfg
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
            server_instance_id: cfg.server_instance_id(),
            worker_count: cfg.worker_count(),
            log_dir: cfg.log_dir().to_string(),
            data_dir: cfg.data_dir().to_string(),
            log_chunk_size: cfg.log_chunk_size(),
            log_batching: cfg.log_batching(),
            routing_mode: cfg.routing_mode(),
            procedure_runtime: cfg.procedure_runtime_for_worker(worker_id),
            worker_identity,
            worker_registry: cfg.worker_registry(),
            async_runtime: cfg.async_runtime(),
        })
    }

    fn build_worker(self) -> RS<WorkerRuntime> {
        WorkerRuntime::new_with_log_batching_and_runtime(
            self.worker_identity,
            self.worker_count,
            self.routing_mode,
            self.log_dir,
            self.data_dir,
            self.log_chunk_size,
            self.log_batching,
            self.procedure_runtime,
            self.worker_registry,
            self.async_runtime,
            self.server_instance_id,
        )
    }
}

fn spawn_stop_bridge(
    name: &'static str,
    stop: Waiter,
    stop_flag: Arc<AtomicBool>,
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
            stop_flag.store(true, Ordering::Relaxed);
            Ok(())
        })
        .map_err(|e| m_error!(EC::ThreadErr, format!("spawn {name} error"), e))
}

fn wait_stop_bridge(name: &'static str, handle: JoinHandle<RS<()>>) -> RS<()> {
    handle
        .join()
        .map_err(|_| m_error!(EC::ThreadErr, format!("join {name} error")))?
}

fn apply_handle_result_to_connection<C: BackendConnection>(
    connection: &mut C,
    inboxes: &[Arc<SegQueue<TransferredConnection>>],
    result: HandleResult,
) -> RS<()> {
    match result {
        HandleResult::Response(payload) => {
            connection.core_mut().write_buf.extend_from_slice(&payload);
        }
        HandleResult::Transfer(transfer) => {
            let stream = connection.take_transfer_stream()?;
            enqueue_transfer(
                inboxes,
                connection.core().conn_id,
                transfer.target_worker(),
                connection.core().remote_addr,
                stream,
                transfer.session_ids().to_vec(),
                Some(transfer.action()),
            )?;
            let core = connection.core_mut();
            core.transferred = true;
            core.state = crate::server::connection_state::ConnectionState::Closing;
            core.write_buf.clear();
        }
    }
    Ok(())
}

fn apply_handle_result<C: BackendConnection>(
    connections: &mut HashMap<u64, C>,
    inboxes: &[Arc<SegQueue<TransferredConnection>>],
    conn_id: u64,
    result: HandleResult,
) -> RS<()> {
    let Some(connection) = connections.get_mut(&conn_id) else {
        return Ok(());
    };
    apply_handle_result_to_connection(connection, inboxes, result)
}

struct FallbackAsyncFuncState {
    next_task_id: u64,
    next_op_id: u64,
    tasks: HashMap<u64, AsyncFuncTask>,
    ready_queue: Arc<SegQueue<u64>>,
    completion_queue: Arc<SegQueue<u64>>,
    op_registry: HashMap<u64, u64>,
}

impl FallbackAsyncFuncState {
    fn new() -> Self {
        Self {
            next_task_id: 1,
            next_op_id: 1,
            tasks: HashMap::new(),
            ready_queue: Arc::new(SegQueue::new()),
            completion_queue: Arc::new(SegQueue::new()),
            op_registry: HashMap::new(),
        }
    }

    fn enqueue_future(&mut self, conn_id: u64, request_id: u64, future: AsyncFuncFuture) {
        let task_id = self.next_task_id;
        self.next_task_id += 1;
        let trace_task_id = new_task_id();
        let _ = TaskContext::new_context(
            trace_task_id,
            format!("tokio_async_func conn={conn_id} req={request_id}"),
            false,
        );
        self.tasks.insert(
            task_id,
            AsyncFuncTask::new(
                conn_id,
                trace_task_id,
                request_id,
                future,
                Arc::new(AtomicBool::new(false)),
            ),
        );
        self.ready_queue.push(task_id);
    }

    fn drain_completions(&mut self) -> bool {
        let mut progressed = false;
        while let Some(op_id) = self.completion_queue.pop() {
            let Some(task_id) = self.op_registry.remove(&op_id) else {
                continue;
            };
            let Some(task) = self.tasks.get(&task_id) else {
                continue;
            };
            if let Some(ctx) = TaskContext::get(task.trace_task_id()) {
                ctx.watch("state", "ready");
                ctx.watch("wake_op_id", &op_id.to_string());
            }
            if !task.queued().swap(true, Ordering::AcqRel) {
                self.ready_queue.push(task_id);
                progressed = true;
            }
        }
        progressed
    }

    fn poll_ready(
        &mut self,
        connections: &mut HashMap<u64, impl BackendConnection>,
        inboxes: &[Arc<SegQueue<TransferredConnection>>],
    ) -> RS<bool> {
        let mut progressed = false;
        while let Some(task_id) = self.ready_queue.pop() {
            let Some(mut task) = self.tasks.remove(&task_id) else {
                continue;
            };
            let trace_task_id = task.trace_task_id();
            trace!(
                task_id,
                conn_id = task.conn_id(),
                request_id = task.request_id(),
                "tokio async task poll begin"
            );
            progressed = true;
            task.clear_queued();
            if let Some(waiting_on) = task.take_waiting_on() {
                self.op_registry.remove(&waiting_on);
            }

            let op_id = self.next_op_id;
            self.next_op_id += 1;
            let waker = waker(Arc::new(AsyncFuncTaskWaker::new(
                op_id,
                self.completion_queue.clone(),
                task.completed().clone(),
            )));
            let mut cx = Context::from_waker(&waker);
            let _guard = PollTaskIdGuard::enter(trace_task_id);
            if let Some(ctx) = TaskContext::get(trace_task_id) {
                ctx.watch("state", "polling");
                ctx.watch("poll_task_id", &task_id.to_string());
            }
            match task.future_mut().poll(&mut cx) {
                Poll::Ready(Ok(result)) => {
                    trace!(
                        task_id,
                        conn_id = task.conn_id(),
                        request_id = task.request_id(),
                        "tokio async task poll ready ok"
                    );
                    TaskContext::remove_context(trace_task_id);
                    apply_handle_result(connections, inboxes, task.conn_id(), result)?;
                }
                Poll::Ready(Err(err)) => {
                    trace!(
                        task_id,
                        conn_id = task.conn_id(),
                        request_id = task.request_id(),
                        err = %err,
                        "tokio async task poll ready err"
                    );
                    TaskContext::remove_context(trace_task_id);
                    if let Some(connection) = connections.get_mut(&task.conn_id()) {
                        let response = encode_merror_response(task.request_id(), &err)?;
                        connection.core_mut().write_buf.extend_from_slice(&response);
                    }
                }
                Poll::Pending => {
                    trace!(
                        task_id,
                        conn_id = task.conn_id(),
                        request_id = task.request_id(),
                        op_id,
                        "tokio async task poll pending"
                    );
                    task.set_waiting_on(op_id);
                    if let Some(ctx) = TaskContext::get(trace_task_id) {
                        ctx.watch("state", "pending");
                        ctx.watch("waiting_waker_op_id", &op_id.to_string());
                    }
                    self.op_registry.insert(op_id, task_id);
                    self.tasks.insert(task_id, task);
                }
            }
        }
        Ok(progressed)
    }
}

impl WorkerTcpBackend {
    /// Starts the backend until shutdown.
    ///
    /// This method keeps the old public entry point stable. It dispatches to
    /// the io_uring implementation on Linux. Select `TokioTcpBackend`
    /// explicitly when the Tokio worker loop is desired on any target.
    pub fn sync_serve(cfg: WorkerTcpServerConfig) -> RS<()> {
        let (_stop_notifier, stop_waiter) = notify_wait();
        Self::sync_serve_with_stop(cfg, stop_waiter)
    }

    /// Internal serve entry that accepts an explicit stop waiter.
    ///
    /// The io_uring backend is Linux-only. The Tokio backend is available as a
    /// separate implementation and bridges the async stop signal into its
    /// worker loop.
    pub fn sync_serve_with_stop(cfg: WorkerTcpServerConfig, stop: Waiter) -> RS<()> {
        Self::sync_serve_with_stop_and_ready(cfg, stop, None)
    }

    pub fn sync_serve_with_stop_and_ready(
        cfg: WorkerTcpServerConfig,
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
    pub fn sync_serve(cfg: WorkerTcpServerConfig) -> RS<()> {
        let (_stop_notifier, stop_waiter) = notify_wait();
        Self::sync_serve_with_stop(cfg, stop_waiter)
    }

    pub fn sync_serve_with_stop(cfg: WorkerTcpServerConfig, stop: Waiter) -> RS<()> {
        Self::sync_serve_with_stop_and_ready(cfg, stop, None)
    }

    pub fn sync_serve_with_stop_and_ready(
        cfg: WorkerTcpServerConfig,
        stop: Waiter,
        ready: Option<Notifier>,
    ) -> RS<()> {
        let stop_flag = Arc::new(AtomicBool::new(false));
        let notifier = spawn_stop_bridge("tokio-stop-bridge", stop, stop_flag.clone())?;
        let result = sync_serve_tokio(cfg, stop_flag, ready);
        wait_stop_bridge("tokio-stop-bridge", notifier)?;
        result
    }
}

fn sync_serve_tokio(
    mut cfg: WorkerTcpServerConfig,
    stop: Arc<AtomicBool>,
    ready: Option<Notifier>,
) -> RS<()> {
    if cfg.worker_count() == 0 {
        return Err(m_error!(EC::ParseErr, "invalid tokio worker count"));
    }
    let listen_addr: SocketAddr = format!("{}:{}", cfg.listen_ip(), cfg.listen_port())
        .parse()
        .map_err(|e| m_error!(EC::ParseErr, "parse tokio tcp listen address error", e))?;

    let conn_id_alloc = Arc::new(AtomicU64::new(1));
    let inboxes: Vec<_> = (0..cfg.worker_count())
        .map(|_| Arc::new(SegQueue::<TransferredConnection>::new()))
        .collect();
    let bus_mailboxes: Vec<_> = (0..cfg.worker_count())
        .map(|_| Arc::new(SegQueue::<Envelope>::new()))
        .collect();
    let listener = match cfg.take_prebound_listener() {
        Some(listener) => listener,
        None => create_listener(listen_addr)?,
    };

    let mut handles = Vec::with_capacity(cfg.worker_count());
    for worker_id in 0..cfg.worker_count() {
        let worker_cfg = WorkerBuildConfig::from_server_config(&cfg, worker_id)?;
        let inbox = inboxes[worker_id].clone();
        let all_inboxes = inboxes.clone();
        let bus_inbox = bus_mailboxes[worker_id].clone();
        let all_bus_mailboxes = bus_mailboxes.clone();
        let conn_id_alloc = conn_id_alloc.clone();
        let stop = stop.clone();
        let listener = listener
            .try_clone()
            .map_err(|e| m_error!(EC::NetErr, "clone tokio tcp listener error", e))?;
        let handle = thread::Builder::new()
            .name(format!("tokio-tcp-worker-{worker_id}"))
            .spawn(move || {
                trace!(worker_id, "tokio worker thread starting");
                listener
                    .set_nonblocking(true)
                    .map_err(|e| m_error!(EC::NetErr, "set tokio listener nonblocking error", e))?;
                let worker = worker_cfg.build_worker()?;
                let message_bus = TokioWorkerMessageBus::new(
                    worker.worker_id(),
                    worker.registry().clone(),
                    all_bus_mailboxes,
                );
                let worker_id = worker.worker_id();
                let server_instance_id = worker.server_instance_id();
                let runtime = CurrentThreadTaskRuntime::new()
                    .map_err(|e| m_error!(EC::TokioErr, "build tokio worker runtime error", e))?;
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
                let result = runtime.block_on(async move {
                    trace!(worker_id, "tokio worker loop entering");
                    let listener = TokioTcpListener::from_std(listener)
                        .map_err(|e| m_error!(EC::NetErr, "convert tokio tcp listener error", e))?;
                    worker.ensure_partition_rpc_handler()?;
                    let (_task_notifier, task_waiter) = notify_wait();
                    let join = spawn_local_task(
                        task_waiter.into(),
                        &format!("tokio_worker_loop_{worker_id}"),
                        run_worker_loop_tokio(
                            worker,
                            listener,
                            inbox,
                            all_inboxes,
                            bus_inbox,
                            message_bus,
                            conn_id_alloc,
                            stop,
                        ),
                    )?;
                    match join.await.map_err(|e| {
                        m_error!(EC::TokioErr, "join tokio worker loop task error", e)
                    })? {
                        Some(result) => result,
                        None => Ok(()),
                    }
                });
                trace!(worker_id, ok = result.is_ok(), "tokio worker loop returned");
                let _ = unregister_worker_message_bus(server_instance_id, worker_id);
                unset_current_message_bus();
                unset_current_worker_local();
                trace!(worker_id, "tokio worker thread exiting");
                result
            })
            .map_err(|e| m_error!(EC::ThreadErr, "spawn tokio worker error", e))?;
        handles.push(handle);
    }

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
    inbox: Arc<SegQueue<TransferredConnection>>,
    inboxes: Vec<Arc<SegQueue<TransferredConnection>>>,
    bus_inbox: Arc<SegQueue<Envelope>>,
    message_bus: Arc<TokioWorkerMessageBus>,
    conn_id_alloc: Arc<AtomicU64>,
    stop: Arc<AtomicBool>,
) -> RS<()> {
    let mut connections = HashMap::<u64, TokioWorkerConnection>::new();
    let mut async_funcs = FallbackAsyncFuncState::new();
    let idle_sleep = Duration::from_millis(1);

    while !stop.load(Ordering::Relaxed) {
        let mut progressed = false;
        trace!(
            worker_id = worker.worker_id(),
            connection_count = connections.len(),
            pending_async_tasks = async_funcs.tasks.len(),
            "tokio worker loop iteration begin"
        );
        progressed |= drain_accepted_connections_tokio(
            &listener,
            &worker,
            &inboxes,
            &mut connections,
            &conn_id_alloc,
        )
        .await?;
        trace!(
            worker_id = worker.worker_id(),
            progressed,
            connection_count = connections.len(),
            pending_async_tasks = async_funcs.tasks.len(),
            "tokio worker loop after accept"
        );
        progressed |=
            drain_transferred_connections_tokio(&worker, inbox.as_ref(), &mut connections)?;
        trace!(
            worker_id = worker.worker_id(),
            progressed,
            connection_count = connections.len(),
            pending_async_tasks = async_funcs.tasks.len(),
            "tokio worker loop after transfer"
        );
        progressed |= drain_message_bus_tokio(bus_inbox.as_ref(), message_bus.as_ref())?;
        trace!(
            worker_id = worker.worker_id(),
            progressed,
            connection_count = connections.len(),
            pending_async_tasks = async_funcs.tasks.len(),
            "tokio worker loop after message bus"
        );
        progressed |= async_funcs.drain_completions();
        trace!(
            worker_id = worker.worker_id(),
            progressed,
            connection_count = connections.len(),
            pending_async_tasks = async_funcs.tasks.len(),
            "tokio worker loop after drain completions"
        );
        progressed |= async_funcs.poll_ready(&mut connections, &inboxes)?;
        trace!(
            worker_id = worker.worker_id(),
            progressed,
            connection_count = connections.len(),
            pending_async_tasks = async_funcs.tasks.len(),
            "tokio worker loop after poll_ready"
        );
        progressed |= drive_connections(&worker, &mut async_funcs, &mut connections, &inboxes)?;
        trace!(
            worker_id = worker.worker_id(),
            progressed,
            connection_count = connections.len(),
            pending_async_tasks = async_funcs.tasks.len(),
            "tokio worker loop after drive_connections"
        );

        if !progressed {
            mudu_sys::task_async::sleep(idle_sleep).await?;
        }
    }
    trace!(
        worker_id = worker.worker_id(),
        remaining_connections = connections.len(),
        pending_async_tasks = async_funcs.tasks.len(),
        "tokio worker loop observed stop"
    );
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

async fn drain_accepted_connections_tokio(
    listener: &TokioTcpListener,
    worker: &WorkerRuntime,
    inboxes: &[Arc<SegQueue<TransferredConnection>>],
    connections: &mut HashMap<u64, TokioWorkerConnection>,
    conn_id_alloc: &AtomicU64,
) -> RS<bool> {
    let mut progressed = false;
    loop {
        match poll_accept_once(listener).await? {
            Some((stream, remote_addr)) => {
                progressed = true;
                route_accepted_connection(
                    worker,
                    inboxes,
                    connections,
                    conn_id_alloc,
                    stream,
                    remote_addr,
                    register_connection_tokio,
                    |stream| {
                        stream.into_std().map_err(|e| {
                            m_error!(EC::NetErr, "convert accepted tokio stream to std error", e)
                        })
                    },
                )?;
            }
            None => break,
        }
    }
    Ok(progressed)
}

fn drain_transferred_connections_tokio(
    worker: &WorkerRuntime,
    inbox: &SegQueue<TransferredConnection>,
    connections: &mut HashMap<u64, TokioWorkerConnection>,
) -> RS<bool> {
    drain_transferred_connections_common(worker, inbox, connections, |connections, connection| {
        connection.stream.set_nonblocking(true).map_err(|e| {
            m_error!(
                EC::NetErr,
                "set transferred tokio stream nonblocking error",
                e
            )
        })?;
        let stream = TokioTcpStream::from_std(connection.stream).map_err(|e| {
            m_error!(
                EC::NetErr,
                "convert transferred std stream to tokio error",
                e
            )
        })?;
        register_connection_tokio(
            connections,
            connection.transfer.conn_id(),
            connection.transfer.remote_addr(),
            stream,
        )
    })
}

fn register_connection_tokio(
    connections: &mut HashMap<u64, TokioWorkerConnection>,
    conn_id: u64,
    remote_addr: SocketAddr,
    stream: TokioTcpStream,
) -> RS<()> {
    stream
        .set_nodelay(true)
        .map_err(|e| m_error!(EC::NetErr, "set tokio connection nodelay error", e))?;
    connections.insert(
        conn_id,
        TokioWorkerConnection {
            core: ConnectionCore::new(conn_id, remote_addr),
            stream: Some(stream),
        },
    );
    Ok(())
}

async fn poll_accept_once(listener: &TokioTcpListener) -> RS<Option<(TokioTcpStream, SocketAddr)>> {
    poll_fn(|cx| match listener.poll_accept(cx) {
        Poll::Ready(Ok(pair)) => Poll::Ready(Ok(Some(pair))),
        Poll::Ready(Err(err)) => Poll::Ready(Err(m_error!(
            EC::NetErr,
            "accept tokio tcp connection error",
            err
        ))),
        Poll::Pending => Poll::Ready(Ok(None)),
    })
    .await
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

fn enqueue_transfer(
    inboxes: &[Arc<SegQueue<TransferredConnection>>],
    conn_id: u64,
    target_worker: usize,
    remote_addr: SocketAddr,
    stream: TcpStream,
    session_ids: Vec<OID>,
    session_open_action: Option<SessionOpenTransferAction>,
) -> RS<()> {
    let target_inbox = inboxes.get(target_worker).ok_or_else(|| {
        m_error!(
            EC::InternalErr,
            format!("route target worker {} is out of range", target_worker)
        )
    })?;
    target_inbox.push(TransferredConnection {
        transfer: ConnectionTransfer::new(
            conn_id,
            target_worker,
            crate::server::connection_state::ConnectionState::Accepted,
            remote_addr,
        ),
        stream,
        session_ids,
        session_open_action,
    });
    Ok(())
}

fn route_accepted_connection<S, C, RegisterLocal, IntoTransfer>(
    worker: &WorkerRuntime,
    inboxes: &[Arc<SegQueue<TransferredConnection>>],
    connections: &mut HashMap<u64, C>,
    conn_id_alloc: &AtomicU64,
    stream: S,
    remote_addr: SocketAddr,
    register_local: RegisterLocal,
    into_transfer: IntoTransfer,
) -> RS<()>
where
    RegisterLocal: FnOnce(&mut HashMap<u64, C>, u64, SocketAddr, S) -> RS<()>,
    IntoTransfer: FnOnce(S) -> RS<TcpStream>,
{
    let conn_id = conn_id_alloc.fetch_add(1, Ordering::Relaxed);
    let target_worker = worker.route_connection(conn_id, remote_addr);
    if target_worker == worker.worker_index() {
        register_local(connections, conn_id, remote_addr, stream)
    } else {
        enqueue_transfer(
            inboxes,
            conn_id,
            target_worker,
            remote_addr,
            into_transfer(stream)?,
            Vec::new(),
            None,
        )
    }
}

fn drain_transferred_connections_common<C, Register>(
    worker: &WorkerRuntime,
    inbox: &SegQueue<TransferredConnection>,
    connections: &mut HashMap<u64, C>,
    mut register: Register,
) -> RS<bool>
where
    C: BackendConnection,
    Register: FnMut(&mut HashMap<u64, C>, TransferredConnection) -> RS<()>,
{
    let mut progressed = false;
    while let Some(connection) = inbox.pop() {
        progressed = true;
        worker.adopt_connection_sessions(connection.transfer.conn_id(), &connection.session_ids)?;
        let conn_id = connection.transfer.conn_id();
        let action = connection.session_open_action;
        register(connections, connection)?;
        if let Some(action) = action {
            let payload = match worker.open_session_with_config(conn_id, action.config()) {
                Ok(session_id) => encode_session_create_response(
                    action.request_id(),
                    &SessionCreateResponse::new(session_id),
                )?,
                Err(err) => encode_merror_response(action.request_id(), &err)?,
            };
            if let Some(registered) = connections.get_mut(&conn_id) {
                registered.core_mut().write_buf.extend_from_slice(&payload);
            }
        }
    }
    Ok(progressed)
}

fn drive_connections<C: BackendConnection>(
    worker: &WorkerRuntime,
    async_funcs: &mut FallbackAsyncFuncState,
    connections: &mut HashMap<u64, C>,
    inboxes: &[Arc<SegQueue<TransferredConnection>>],
) -> RS<bool> {
    let mut progressed = false;
    let conn_ids: Vec<u64> = connections.keys().copied().collect();
    let mut closed = Vec::new();

    for conn_id in conn_ids {
        trace!(conn_id, "tokio drive connection begin");
        let Some(connection) = connections.get_mut(&conn_id) else {
            continue;
        };
        progressed |= connection.write_pending()?;
        trace!(
            conn_id,
            state = ?connection.core().state,
            write_buf_len = connection.core().write_buf.len(),
            read_buf_len = connection.core().read_buf.len(),
            "tokio drive connection after write_pending"
        );
        let connection_progress = read_and_dispatch(worker, async_funcs, connection, inboxes)?;
        progressed |= connection_progress;
        trace!(
            conn_id,
            connection_progress,
            state = ?connection.core().state,
            write_buf_len = connection.core().write_buf.len(),
            read_buf_len = connection.core().read_buf.len(),
            "tokio drive connection after read_and_dispatch"
        );
        if connection.core().state == crate::server::connection_state::ConnectionState::Closing
            && connection.core().write_buf.is_empty()
        {
            closed.push((conn_id, connection.core().transferred));
        }
    }

    for (conn_id, transferred) in closed {
        if !transferred {
            worker.close_connection_sessions(conn_id)?;
        }
        connections.remove(&conn_id);
    }
    Ok(progressed)
}

fn read_and_dispatch<C: BackendConnection>(
    worker: &WorkerRuntime,
    async_funcs: &mut FallbackAsyncFuncState,
    connection: &mut C,
    inboxes: &[Arc<SegQueue<TransferredConnection>>],
) -> RS<bool> {
    let mut progressed = connection.read_available()?;

    while let Some((frame, consumed)) = try_decode_next_frame(&connection.core().read_buf)? {
        progressed = true;
        let response = dispatch_frame(worker, connection.core().conn_id, async_funcs, &frame);
        connection.core_mut().read_buf.drain(0..consumed);
        match response {
            Ok(Some(result)) => {
                apply_handle_result_to_connection(connection, inboxes, result)?;
                if connection.core().transferred {
                    return Ok(true);
                }
            }
            Ok(None) => {}
            Err(err) => {
                let payload = encode_merror_response(frame.header().request_id(), &err)?;
                connection.core_mut().write_buf.extend_from_slice(&payload);
            }
        }
    }
    Ok(progressed)
}

fn dispatch_frame(
    worker: &WorkerRuntime,
    conn_id: u64,
    async_funcs: &mut FallbackAsyncFuncState,
    frame: &Frame,
) -> RS<Option<HandleResult>> {
    let request_id = frame.header().request_id();
    trace!(conn_id, request_id, "tokio dispatch frame begin");
    let worker = worker.clone();
    let frame = frame.clone();
    let mut future = Box::pin(async move {
        mudu_utils::scoped_task_trace!();
        dispatch_frame_async(&worker, conn_id, &frame).await
    });
    let waker = waker(Arc::new(AsyncFuncTaskWaker::new(
        0,
        Arc::new(SegQueue::new()),
        Arc::new(AtomicBool::new(false)),
    )));
    let mut cx = Context::from_waker(&waker);
    match future.as_mut().poll(&mut cx) {
        Poll::Ready(Ok(result)) => {
            trace!(conn_id, request_id, "tokio dispatch frame ready ok");
            Ok(Some(result))
        }
        Poll::Ready(Err(err)) => {
            trace!(conn_id, request_id, err = %err, "tokio dispatch frame ready err");
            Err(err)
        }
        Poll::Pending => {
            trace!(conn_id, request_id, "tokio dispatch frame pending");
            async_funcs.enqueue_future(conn_id, request_id, future);
            Ok(None)
        }
    }
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
