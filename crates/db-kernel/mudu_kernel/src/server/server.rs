use crate::server::async_func_runtime::AsyncFuncInvokerPtr;
use crate::server::async_func_task::HandleResult;
use mudu_sys::contract::async_io_provider::AsyncIoProvider;

use crate::server::frame_dispatch::{dispatch_frame_async, try_decode_next_frame};
use crate::server::fs_gc::FS_GC_INTERVAL;
use crate::server::message_bus_api::{
    register_worker_message_bus, set_current_message_bus, unregister_worker_message_bus,
    unset_current_message_bus, EndpointId, Envelope, MessageBus, MessageBusRef, MessageId,
    OnRecvCallback, OutgoingMessage, RecvFilter, ServerInstanceId, SubscriptionId,
};
use crate::server::message_bus_state::WorkerMessageBusState;
use crate::server::session_bound_worker_runtime::{
    as_worker_local_ref, new_session_bound_worker_runtime,
};
use crate::server::worker::{WorkerRuntime, WorkerRuntimeParams};
use crate::server::worker_local::{set_current_worker_local, unset_current_worker_local};
use crate::server::worker_registry::{WorkerIdentity, WorkerRegistry};
use crate::server::worker_storage::DIRTY_PAGE_FLUSH_INTERVAL;
use crate::wal::worker_log::{scan_valid_frame_prefix, ChunkedWorkerLogBackend, WorkerLogBackend};
use crate::wal::worker_log::{WalSyncPolicy, WorkerLogBatching};
use crate::wal::xl_batch::decode_xl_batches_with_pending;
use async_trait::async_trait;
use crossbeam_queue::SegQueue;

use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_contract::protocol::encode_merror_response;
use mudu_sys::net::{AsyncTcpListener, AsyncTcpStream};
use mudu_sys::sync::async_::stop_flag::{stop_channel, StopRx, StopTx};
use mudu_sys::tokio;
use mudu_sys::tokio::io::{AsyncReadExt, AsyncWriteExt};
use mudu_sys::tokio::sync::Notify;
use mudu_utils::notifier::{notify_wait, Notifier, Waiter};
use mudu_utils::scoped_task_trace;
use mudu_utils::task_async::{
    build_current_thread_runtime, spawn_local_detached, spawn_local_task, CurrentThreadTaskRuntime,
};

use mudu_sys::net::sync::StdTcpListener;
use mudu_sys::sync::SMutex;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{atomic::AtomicBool, Arc};
use std::time::Duration;

use crate::server::server_launch::{ServerLaunch, WorkerTcpBackendConfig};
use mudu_sys::task::sync::SJoinHandle;

use tracing::{error, trace, warn};

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
            let mut state = self.state.lock().map_err(|_| {
                mudu_error!(ErrorCode::Internal, "tokio message bus state lock poisoned")
            })?;
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
            .ok_or_else(|| {
                mudu_error!(
                    ErrorCode::EntityNotFound,
                    format!("no such worker id {}", endpoint)
                )
            })
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
            dst,
            message.kind(),
            message.payload_owned(),
            message.delivery(),
        );
        let target_worker = self.route_worker_index(dst)?;
        let Some(mailbox) = self.mailboxes.get(target_worker) else {
            return Err(mudu_error!(
                ErrorCode::Internal,
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
            let mut state = self.state.lock().map_err(|_| {
                mudu_error!(ErrorCode::Internal, "tokio message bus state lock poisoned")
            })?;
            if let Some(envelope) = state.try_take_message(&filter) {
                return Ok(envelope);
            }
            state.register_waiter(filter)
        };
        receiver.wait().await?.ok_or_else(|| {
            mudu_error!(
                ErrorCode::ChannelClosed,
                "message bus waiter dropped before delivery"
            )
        })
    }

    fn on_recv_callback(&self, filter: RecvFilter, callback: OnRecvCallback) -> RS<SubscriptionId> {
        let (callback_id, maybe_envelope) = {
            let mut state = self.state.lock().map_err(|_| {
                mudu_error!(ErrorCode::Internal, "tokio message bus state lock poisoned")
            })?;
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
        let mut state = self.state.lock().map_err(|_| {
            mudu_error!(ErrorCode::Internal, "tokio message bus state lock poisoned")
        })?;
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
    wal_sync_policy: WalSyncPolicy,
    procedure_runtime: Option<AsyncFuncInvokerPtr>,
    worker_identity: WorkerIdentity,
    worker_registry: Arc<WorkerRegistry>,
    async_runtime: Option<Arc<dyn AsyncIoProvider>>,
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
                mudu_error!(
                    ErrorCode::EntityNotFound,
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
            wal_sync_policy: deps.wal_sync_policy(),
            procedure_runtime: deps.procedure_runtime_for_worker(worker_id),
            worker_identity,
            worker_registry: deps.worker_registry(),
            async_runtime: deps.async_runtime(),
        })
    }

    async fn build_worker(self) -> RS<WorkerRuntime> {
        WorkerRuntime::new(WorkerRuntimeParams {
            identity: self.worker_identity,
            worker_count: self.worker_count,
            log_dir: self.log_dir,
            data_dir: self.data_dir,
            log_chunk_size: self.log_chunk_size,
            log_batching: self.log_batching,
            wal_sync_policy: self.wal_sync_policy,
            procedure_runtime: self.procedure_runtime,
            registry: self.worker_registry,
            async_runtime: self.async_runtime,
            server_instance_id: self.server_instance_id,
        })
        .await
    }
}

fn spawn_stop_bridge(
    name: &'static str,
    stop: Waiter,
    stop_flag: Arc<AtomicBool>,
    service_ready: Arc<AtomicBool>,
    stop_tx: StopTx,
) -> RS<SJoinHandle<RS<()>>> {
    mudu_sys::task::sync::spawn_thread_named(name, move || {
        let runtime = build_current_thread_runtime().map_err(|e| {
            mudu_error!(
                ErrorCode::Tokio,
                format!("create runtime for {name} error"),
                e
            )
        })?;
        trace!(bridge = name, "tokio stop bridge waiting for stop");
        runtime.block_on(stop.wait());
        trace!(bridge = name, "tokio stop bridge observed stop");
        service_ready.store(false, Ordering::Relaxed);
        stop_flag.store(true, Ordering::Relaxed);
        stop_tx.stop();
        Ok(())
    })
}

fn wait_stop_bridge(name: &'static str, handle: SJoinHandle<RS<()>>) -> RS<()> {
    handle
        .join()
        .map_err(|_| mudu_error!(ErrorCode::Thread, format!("join {name} error")))?
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
            crate::server::server_iouring::sync_serve_iouring(cfg, stop, ready)
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
        return Err(mudu_error!(ErrorCode::Parse, "invalid tokio worker count"));
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
        let listener = if let Some(prebound) = cfg.take_prebound_listener(worker_id) {
            prebound
        } else {
            let worker_port = cfg.cfg().listen_port_for_worker(worker_id)?;
            let listen_addr: SocketAddr = format!("{}:{}", cfg.cfg().listen_ip(), worker_port)
                .parse()
                .map_err(|e| {
                    mudu_error!(
                        ErrorCode::Parse,
                        format!("parse tokio tcp listen address error: {}", worker_port),
                        e
                    )
                })?;
            create_listener(listen_addr)?
        };
        let handle = mudu_sys::task::sync::spawn_thread_named(
            format!("tokio-tcp-worker-{worker_id}"),
            move || {
                let runtime = CurrentThreadTaskRuntime::new().map_err(|e| {
                    mudu_error!(ErrorCode::Tokio, "build tokio worker runtime error", e)
                })?;
                let result = runtime.block_on(async move {
                    trace!(worker_id, "tokio worker thread starting");
                    listener.set_nonblocking(true).map_err(|e| {
                        mudu_error!(
                            ErrorCode::Network,
                            "set tokio listener nonblocking error",
                            e
                        )
                    })?;
                    let worker = worker_cfg.build_worker().await?;
                    worker.initialize().await.map_err(|e| {
                        mudu_error!(ErrorCode::Storage, "initialize worker failed", e)
                    })?;
                    worker.bootstrap_storage_async().await.map_err(|e| {
                        mudu_error!(ErrorCode::Storage, "bootstrap worker storage failed", e)
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
                    set_current_worker_local(as_worker_local_ref(
                        new_session_bound_worker_runtime(worker.clone(), 0),
                    ));
                    let message_bus_ref = message_bus.bus_ref();
                    set_current_message_bus(message_bus_ref.clone());
                    register_worker_message_bus(
                        server_instance_id,
                        worker.worker_id(),
                        &message_bus_ref,
                    )?;
                    trace!(worker_id, "tokio worker loop entering");
                    let listener = adopt_worker_listener(listener).await?;
                    worker.ensure_partition_rpc_handler()?;
                    recover_worker_log_tokio(&worker).await?;
                    worker.fs_gc_recover_scan().await?;
                    let (_gc_task_notifier, gc_task_waiter) = notify_wait();
                    let fs_gc = worker.fs_gc();
                    let gc_stop_rx = stop_rx.clone();
                    let gc_join = spawn_local_task(
                        gc_task_waiter,
                        &format!("fs_gc_loop_{worker_id}"),
                        async move { fs_gc.gc_loop(FS_GC_INTERVAL, gc_stop_rx).await },
                    )?;
                    // WAL group-commit flush driver. It is stopped only after
                    // the worker loop has drained, so no commit can enqueue
                    // behind the final force-flush round.
                    let (wal_flush_stop_tx, wal_flush_stop_rx) = stop_channel();
                    let wal_flush_join = match worker.worker_log()? {
                        Some(wal_log) => {
                            let (_wal_task_notifier, wal_task_waiter) = notify_wait();
                            Some(spawn_local_task(
                                wal_task_waiter,
                                &format!("wal_flush_loop_{worker_id}"),
                                run_worker_wal_flush_loop(wal_log, wal_flush_stop_rx),
                            )?)
                        }
                        None => None,
                    };
                    // WAL periodic-fsync driver (own task: an in-flight fsync
                    // must never block the flush loop's write rounds). It is
                    // stopped after the flush loop, so its final forced fsync
                    // covers whatever the flush loop's final force-flush left
                    // dirty in periodic sync mode.
                    let (wal_fsync_stop_tx, wal_fsync_stop_rx) = stop_channel();
                    let wal_fsync_join = match worker.worker_log()? {
                        Some(wal_log) => {
                            let (_fsync_task_notifier, fsync_task_waiter) = notify_wait();
                            Some(spawn_local_task(
                                fsync_task_waiter,
                                &format!("wal_fsync_loop_{worker_id}"),
                                run_worker_wal_fsync_loop(wal_log, wal_fsync_stop_rx),
                            )?)
                        }
                        None => None,
                    };
                    // Deferred data-page flush driver (WAL-first dirty
                    // pages): writes back dirty time-series pages in batches
                    // so consecutive row writes coalesce into one page write.
                    // It is stopped together with the WAL flush driver.
                    let (page_flush_stop_tx, page_flush_stop_rx) = stop_channel();
                    let (_page_task_notifier, page_task_waiter) = notify_wait();
                    let page_flush_join = spawn_local_task(
                        page_task_waiter,
                        &format!("page_flush_loop_{worker_id}"),
                        run_worker_page_flush_loop(worker.clone(), page_flush_stop_rx),
                    )?;
                    let (_task_notifier, task_waiter) = notify_wait();
                    let join = spawn_local_task(
                        task_waiter,
                        &format!("tokio_worker_loop_{worker_id}"),
                        run_worker_loop_tokio(TokioWorkerLoopArgs {
                            worker,
                            listener,
                            bus_inbox,
                            message_bus,
                            bus_wake,
                            conn_id_alloc,
                            stop,
                            stop_rx,
                            service_ready,
                            conn_tasks: conn_tasks.clone(),
                            rpc_ready_tx: Some(rpc_ready_tx),
                        }),
                    )?;
                    let _ = started_tx.send(Ok(()));
                    let loop_result = match join.await.map_err(|e| {
                        mudu_error!(ErrorCode::Tokio, "join tokio worker loop task error", e)
                    })? {
                        Some(result) => result,
                        None => Ok(()),
                    };
                    // The message bus and worker-local must stay registered for
                    // the whole lifetime of the worker loop above: the loop and
                    // the partition rpc handlers resolve them from this thread
                    // while serving requests (cross-worker reads and commit
                    // handoffs). Tear them down only after the loop exited.
                    let _ = unregister_worker_message_bus(server_instance_id, worker_id);
                    unset_current_message_bus();
                    unset_current_worker_local();
                    wal_flush_stop_tx.stop();
                    let wal_flush_result = match wal_flush_join {
                        Some(wal_flush_join) => match wal_flush_join.await.map_err(|e| {
                            mudu_error!(ErrorCode::Tokio, "join wal flush loop task error", e)
                        })? {
                            Some(result) => result,
                            None => Ok(()),
                        },
                        None => Ok(()),
                    };
                    wal_fsync_stop_tx.stop();
                    let wal_fsync_result = match wal_fsync_join {
                        Some(wal_fsync_join) => match wal_fsync_join.await.map_err(|e| {
                            mudu_error!(ErrorCode::Tokio, "join wal fsync loop task error", e)
                        })? {
                            Some(result) => result,
                            None => Ok(()),
                        },
                        None => Ok(()),
                    };
                    page_flush_stop_tx.stop();
                    let page_flush_result = match page_flush_join.await.map_err(|e| {
                        mudu_error!(ErrorCode::Tokio, "join page flush loop task error", e)
                    })? {
                        Some(result) => result,
                        None => Ok(()),
                    };
                    let gc_result = match gc_join.await.map_err(|e| {
                        mudu_error!(ErrorCode::Tokio, "join fs gc loop task error", e)
                    })? {
                        Some(result) => result,
                        None => Ok(()),
                    };
                    loop_result
                        .and(gc_result)
                        .and(wal_flush_result)
                        .and(wal_fsync_result)
                        .and(page_flush_result)
                });
                trace!(worker_id, ok = result.is_ok(), "tokio worker loop returned");

                trace!(worker_id, "tokio worker thread exiting");
                result
            },
        )?;
        handles.push(handle);
    }
    drop(started_tx);
    drop(rpc_ready_tx);

    for _ in 0..cfg.cfg().worker_count() {
        let started = started_rx.recv().map_err(|_| {
            mudu_error!(
                ErrorCode::Thread,
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
            mudu_error!(
                ErrorCode::Thread,
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
            .map_err(|_| mudu_error!(ErrorCode::Thread, "join tokio worker error"))?;
        trace!(worker_id, ok = result.is_ok(), "joined tokio worker");
        result?;
    }
    Ok(())
}

struct TokioWorkerLoopArgs {
    worker: WorkerRuntime,
    listener: AsyncTcpListener,
    bus_inbox: Arc<SegQueue<Envelope>>,
    message_bus: Arc<TokioWorkerMessageBus>,
    bus_wake: Arc<Notify>,
    conn_id_alloc: Arc<AtomicU64>,
    stop: Arc<AtomicBool>,
    stop_rx: StopRx,
    service_ready: Arc<AtomicBool>,
    conn_tasks: TokioConnTaskState,
    rpc_ready_tx: Option<mpsc::Sender<RS<()>>>,
}

async fn run_worker_loop_tokio(args: TokioWorkerLoopArgs) -> RS<()> {
    let TokioWorkerLoopArgs {
        worker,
        listener,
        bus_inbox,
        message_bus,
        bus_wake,
        conn_id_alloc,
        stop,
        mut stop_rx,
        service_ready,
        conn_tasks,
        rpc_ready_tx,
    } = args;
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
                    .map_err(|err| mudu_error!(ErrorCode::Network, "accept tokio tcp connection error", err))?;
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
    let _ = mudu_sys::timeout(Duration::from_secs(3), conn_tasks.wait_drained()).await;
    trace!(
        worker_id = worker.worker_id(),
        "tokio worker loop observed stop"
    );
    Ok(())
}

async fn recover_worker_log_tokio(worker: &WorkerRuntime) -> RS<()> {
    let Some(log) = worker.worker_log()? else {
        return Ok(());
    };
    let fs = log.fs();
    let chunk_paths = log.chunk_paths_sorted().await?;
    // Multi-part batches can straddle chunk boundaries; keep the
    // not-yet-terminated frames across chunks and drop whatever is left
    // unterminated at end-of-log (the writer crashed mid-batch). Each chunk
    // is decoded up to its longest valid frame prefix: a crash can leave an
    // un-fsynced tail behind, and the frame CRCs mark exactly where valid
    // data ends.
    let mut pending_frames = Vec::new();
    let mut pending_start_lsn = None;
    for path in chunk_paths {
        let bytes = fs.read_all(&path).await?;
        if bytes.is_empty() {
            continue;
        }
        let prefix = scan_valid_frame_prefix(&bytes);
        if let Some(reason) = &prefix.corrupt_reason {
            warn!(
                path = %path.display(),
                truncated_bytes = bytes.len() - prefix.valid_len,
                reason = %reason,
                "dropping un-persisted worker log chunk tail during recovery"
            );
        }
        let batches = decode_xl_batches_with_pending(
            &prefix.frames,
            &mut pending_frames,
            &mut pending_start_lsn,
        )?;
        for batch in batches {
            worker.replay_log_batch(batch).await?;
        }
    }
    if !pending_frames.is_empty() {
        warn!(
            pending_frames = pending_frames.len(),
            "dropping unterminated log entry at end of worker log"
        );
    }
    Ok(())
}

/// Tokio WAL group-commit flush driver. Enqueued commit batches are written
/// and fsynced in batches: an enqueue that satisfies the batching watermarks
/// wakes the driver immediately, otherwise the driver re-checks the queue
/// every `flush_idle_interval`. On stop it force-flushes whatever remains so
/// no queued commit is left waiting. In periodic sync mode this loop stays
/// write-only; the fsync schedule lives in [`run_worker_wal_fsync_loop`] so
/// an in-flight fsync never blocks write flushes.
async fn run_worker_wal_flush_loop(log: ChunkedWorkerLogBackend, mut stop_rx: StopRx) -> RS<()> {
    loop {
        if stop_rx.is_stopped() {
            break;
        }
        log.flush_pending_batches().await?;
        tokio::select! {
            wait_result = log.wait_flush_trigger() => {
                wait_result?;
            }
            _ = mudu_sys::task::async_::sleep(log.flush_idle_interval()) => {}
            changed = stop_rx.changed() => {
                if !changed || stop_rx.is_stopped() {
                    break;
                }
            }
        }
    }
    log.force_flush_log_async().await?;
    Ok(())
}

/// Tokio WAL periodic-fsync driver. In periodic sync mode it fsyncs the
/// dirty WAL chunks once the sync interval elapses, in its own task so a
/// ~10ms fsync never blocks the flush loop's write rounds. On stop it forces
/// a final fsync so a clean stop never leaves acknowledged commits
/// un-fsynced. In Commit mode `maybe_periodic_fsync` is a no-op and the
/// loop just idles.
async fn run_worker_wal_fsync_loop(log: ChunkedWorkerLogBackend, mut stop_rx: StopRx) -> RS<()> {
    let interval = log
        .periodic_fsync_interval()
        .unwrap_or_else(|| log.flush_idle_interval())
        .min(log.flush_idle_interval());
    loop {
        if stop_rx.is_stopped() {
            break;
        }
        log.maybe_periodic_fsync().await?;
        tokio::select! {
            _ = mudu_sys::task::async_::sleep(interval) => {}
            changed = stop_rx.changed() => {
                if !changed || stop_rx.is_stopped() {
                    break;
                }
            }
        }
    }
    log.fsync_unsynced_paths().await?;
    Ok(())
}

/// Writes back dirty time-series data pages of this worker's relations and
/// meta catalogs. Shared by the tokio flush loop and the io_uring ring
/// loop's periodic flush round.
pub(crate) async fn flush_worker_dirty_pages(worker: &WorkerRuntime) -> RS<()> {
    worker.storage().flush_dirty_pages_async().await?;
    worker.meta_mgr().flush_dirty_pages().await?;
    Ok(())
}

/// Tokio deferred data-page flush driver. Every `DIRTY_PAGE_FLUSH_INTERVAL`
/// it writes back the worker's dirty time-series pages; on stop it flushes
/// whatever remains so the data files are clean before the worker tears
/// down. A failed round is logged and retried at the next interval because
/// dirty marks survive a failed flush; the final flush result propagates.
async fn run_worker_page_flush_loop(worker: WorkerRuntime, mut stop_rx: StopRx) -> RS<()> {
    loop {
        if stop_rx.is_stopped() {
            break;
        }
        if let Err(err) = flush_worker_dirty_pages(&worker).await {
            error!(
                worker_id = worker.worker_id(),
                "page flush round failed, {}", err
            );
        }
        tokio::select! {
            _ = mudu_sys::task::async_::sleep(DIRTY_PAGE_FLUSH_INTERVAL) => {}
            changed = stop_rx.changed() => {
                if !changed || stop_rx.is_stopped() {
                    break;
                }
            }
        }
    }
    flush_worker_dirty_pages(&worker).await
}

async fn handle_tokio_connection(
    worker: WorkerRuntime,
    mut stream: AsyncTcpStream,
    conn_id: u64,
    remote_addr: SocketAddr,
    stop: Arc<AtomicBool>,
    mut stop_rx: StopRx,
    service_ready: Arc<AtomicBool>,
) -> RS<()> {
    scoped_task_trace!();
    stream
        .set_nodelay(true)
        .map_err(|e| mudu_error!(ErrorCode::Network, "set tokio connection nodelay error", e))?;
    let mut read_buf: Vec<u8> = Vec::with_capacity(8192);
    let mut chunk = vec![0u8; 8192];
    loop {
        crate::server::stage_stats::dump_if_due(worker.worker_id());
        if stop.load(Ordering::Relaxed) || stop_rx.is_stopped() {
            break;
        }
        let read = tokio::select! {
            read_result = stream.read(&mut chunk) => {
                read_result.map_err(|e| mudu_error!(ErrorCode::Network, "read tokio tcp request error", e))?
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
                let err = mudu_error!(ErrorCode::Internal, "server is not ready");
                let payload = encode_merror_response(frame.header().request_id(), &err)?;
                stream.write_all(&payload).await.map_err(|e| {
                    mudu_error!(ErrorCode::Network, "write tokio tcp response error", e)
                })?;
                continue;
            }
            match dispatch_frame_async(&worker, conn_id, &frame).await {
                Ok(HandleResult::Response(payload)) => {
                    stream.write_all(&payload).await.map_err(|e| {
                        mudu_error!(ErrorCode::Network, "write tokio tcp response error", e)
                    })?;
                }
                Err(err) => {
                    let payload = encode_merror_response(frame.header().request_id(), &err)?;
                    stream.write_all(&payload).await.map_err(|e| {
                        mudu_error!(ErrorCode::Network, "write tokio tcp response error", e)
                    })?;
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

fn create_listener(listen_addr: SocketAddr) -> RS<StdTcpListener> {
    mudu_sys::net::sync::bind_tcp(listen_addr)
}

/// Converts the prebound synchronous worker listener into the async listener
/// the tokio worker loop accepts connections on.
///
/// Native backend: adopts the real OS socket via `into_inner()`.
#[cfg(not(feature = "ds"))]
async fn adopt_worker_listener(listener: StdTcpListener) -> RS<AsyncTcpListener> {
    AsyncTcpListener::from_std(listener.into_inner())
        .map_err(|e| mudu_error!(ErrorCode::Network, "convert tokio tcp listener error", e))
}

/// Converts the prebound synchronous worker listener into the async listener
/// the tokio worker loop accepts connections on.
///
/// Simulation backend: a simulated listener cannot be turned into a real OS
/// socket, so the worker releases the simulated synchronous port reservation
/// and rebinds the same address on the simulated async listener that
/// simulated async clients connect to.
#[cfg(feature = "ds")]
async fn adopt_worker_listener(listener: StdTcpListener) -> RS<AsyncTcpListener> {
    let addr = listener.local_addr().map_err(|e| {
        mudu_error!(
            ErrorCode::Network,
            "read simulated listener local address error",
            e
        )
    })?;
    // The simulation shares one port space between sync and async listeners,
    // so the sync reservation must be released before the async rebind.
    drop(listener);
    AsyncTcpListener::bind(addr).await.map_err(|e| {
        mudu_error!(
            ErrorCode::Network,
            "bind simulated async tcp listener error",
            e
        )
    })
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
