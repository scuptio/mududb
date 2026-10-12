use super::message_bus::TokioWorkerMessageBus;
use super::*;

#[derive(Clone)]
pub(crate) struct TokioConnTaskState {
    active: Arc<std::sync::atomic::AtomicU64>,
    drained: Arc<Notify>,
}

impl TokioConnTaskState {
    pub(crate) fn new() -> Self {
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

pub(crate) struct TokioWorkerLoopArgs {
    pub(crate) worker: WorkerRuntime,
    pub(crate) listener: AsyncTcpListener,
    pub(crate) bus_inbox: Arc<SegQueue<Envelope>>,
    pub(crate) message_bus: Arc<TokioWorkerMessageBus>,
    pub(crate) bus_wake: Arc<Notify>,
    pub(crate) conn_id_alloc: Arc<AtomicU64>,
    pub(crate) stop: Arc<AtomicBool>,
    pub(crate) stop_rx: StopRx,
    pub(crate) service_ready: Arc<AtomicBool>,
    pub(crate) conn_tasks: TokioConnTaskState,
    pub(crate) rpc_ready_tx: Option<mpsc::Sender<RS<()>>>,
}

pub(crate) async fn run_worker_loop_tokio(args: TokioWorkerLoopArgs) -> RS<()> {
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
        crate::stage_stats::dump_if_due(worker.worker_id());
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
