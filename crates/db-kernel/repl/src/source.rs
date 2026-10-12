//! Leader side of the replication stream: a node-level replication listener
//! that serves `ReplSubscribe`, `ReplAck` and `ReplHealFetch` frames of
//! follower workers. The listener runs on its own thread with a
//! current-thread runtime and is spawned only when replication is configured.

use crate::raft_service::raft_frame_to_msg;
use crate::{
    raft_for, read_next_frame, record_repl_ack, register_repl_subscriber, repl_worker,
    unregister_repl_subscriber, ReplWorkerState,
};
use ::catalog::contract::message_bus_api::ServerInstanceId;
use ::catalog::repl_cfg::ReplNodeCfg;
use ::wal::log_frame::frame_lsn;
use ::wal::worker_log::{scan_valid_frame_prefix, ChunkedWorkerLogBackend, WorkerLogBackend};
use ::worker::worker::adopt_worker_listener;
use ::worker::worker::WorkerRuntime;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_contract::protocol::{
    decode_repl_ack_frame, decode_repl_heal_fetch_request, decode_repl_snapshot_fetch_request,
    decode_repl_subscribe_request, encode_merror_response, encode_merror_response_with_leader_hint,
    encode_repl_batch_frame, encode_repl_heal_response_frame, encode_repl_snapshot_offer_frame,
    encode_repl_snapshot_resp_frame, encode_repl_subscribe_accept_frame, LeaderHint, MessageType,
    ReplBatchFrame, ReplHealRespFrame, ReplSnapshotFetchReq, ReplSnapshotOfferFrame,
    ReplSnapshotRespFrame, ReplSubscribeAcceptFrame, ReplSubscribeReq,
};
use mudu_sys::contract::async_file::AsyncFile;
use mudu_sys::contract::file_options::FileOptions;
use mudu_sys::net::sync::StdTcpListener;
use mudu_sys::net::AsyncTcpStream;
use mudu_sys::sync::async_::stop_flag::{stop_channel, StopRx};
use mudu_sys::task::sync::SJoinHandle;
use mudu_sys::tokio;
use mudu_sys::tokio::io::AsyncWriteExt;
use mudu_sys::tokio::sync::mpsc::unbounded_channel;
use mudu_sys::tokio::sync::Notify;
use mudu_utils::task_async::{spawn_local_detached, CurrentThreadTaskRuntime};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tracing::{trace, warn};

/// How often a caught-up subscription re-polls the worker log for newly
/// flushed frames. The WAL flush driver appends asynchronously, so polling
/// is sufficient; the commit path is deliberately not hooked.
const REPL_POLL_INTERVAL: Duration = Duration::from_millis(15);

/// Maximum bytes read from a WAL chunk in one streaming step. Larger frames
/// grow the window dynamically (see [`ChunkCursor::next_batch`]).
const REPL_READ_SLICE_CAP: u64 = 4 * 1024 * 1024;

/// Spawns the replication source thread: one thread with a current-thread
/// tokio runtime accepting replication connections on the pre-bound listener
/// until `stop_rx` fires.
pub fn spawn_repl_source(
    server_instance_id: ServerInstanceId,
    listener: StdTcpListener,
    repl_cfg: ReplNodeCfg,
    stop_rx: StopRx,
) -> RS<SJoinHandle<RS<()>>> {
    mudu_sys::task::sync::spawn_thread_named("repl-source".to_string(), move || {
        let runtime = CurrentThreadTaskRuntime::new()
            .map_err(|e| mudu_error!(ErrorCode::Tokio, "build repl source runtime error", e))?;
        runtime.block_on(async move {
            run_repl_source(server_instance_id, listener, repl_cfg, stop_rx).await
        })
    })
}

async fn run_repl_source(
    server_instance_id: ServerInstanceId,
    listener: StdTcpListener,
    repl_cfg: ReplNodeCfg,
    mut stop_rx: StopRx,
) -> RS<()> {
    listener
        .set_nonblocking(true)
        .map_err(|e| mudu_error!(ErrorCode::Network, "set repl listener nonblocking error", e))?;
    let listener = adopt_worker_listener(listener).await?;
    trace!(port = repl_cfg.listen_port(), "replication listener ready");
    loop {
        tokio::select! {
            accept_result = listener.accept() => {
                let (stream, remote_addr) = accept_result
                    .map_err(|e| mudu_error!(ErrorCode::Network, "accept repl connection error", e))?;
                trace!(remote = %remote_addr, "accepted replication connection");
                let conn_cfg = repl_cfg.clone();
                let conn_stop_rx = stop_rx.clone();
                let _ = spawn_local_detached("repl_connection", async move {
                    let result =
                        handle_repl_conn(server_instance_id, stream, conn_cfg, conn_stop_rx).await;
                    if let Err(err) = &result {
                        warn!(remote = %remote_addr, "replication connection closed: {}", err);
                    }
                    result
                });
            }
            changed = stop_rx.changed() => {
                if !changed || stop_rx.is_stopped() {
                    break;
                }
            }
        }
    }
    Ok(())
}

/// Handles one replication connection until it closes or the server stops.
async fn handle_repl_conn(
    server_instance_id: ServerInstanceId,
    mut stream: AsyncTcpStream,
    repl_cfg: ReplNodeCfg,
    mut stop_rx: StopRx,
) -> RS<()> {
    stream
        .set_nodelay(true)
        .map_err(|e| mudu_error!(ErrorCode::Network, "set repl connection nodelay error", e))?;
    let mut read_buf: Vec<u8> = Vec::with_capacity(8192);
    let mut chunk = vec![0u8; 8192];
    while let Some(frame) =
        read_next_frame(&mut stream, &mut read_buf, &mut chunk, &mut stop_rx).await?
    {
        // Raft traffic shares the replication channel: deliver and read on.
        if let Some((src, msg)) = raft_frame_to_msg(&frame)? {
            if let Some(raft) = raft_for(server_instance_id)? {
                trace!(src, msg_type = ?frame.header().message_type(), "raft frame delivered");
                raft.deliver(src, msg)?;
            } else {
                trace!(
                    "dropping raft frame from node {}: raft service is not running",
                    src
                );
            }
            continue;
        }
        match frame.header().message_type() {
            MessageType::ReplSubscribe => {
                let request = decode_repl_subscribe_request(&frame)?;
                return run_subscription(
                    server_instance_id,
                    stream,
                    read_buf,
                    repl_cfg,
                    request,
                    frame.header().request_id(),
                    stop_rx,
                )
                .await;
            }
            MessageType::ReplAck => {
                // A standalone ack carries no subscriber identity; only acks
                // inside a subscription (where the subscriber is known) are
                // recorded. Followers never send standalone acks.
                trace!("ignoring replication ack outside a subscription");
            }
            MessageType::ReplHealFetch => {
                let fetch = decode_repl_heal_fetch_request(&frame)?;
                let handle = repl_worker(server_instance_id, fetch.worker_id())?;
                let frames = gather_frames_for_heal(
                    handle.worker(),
                    handle.state(),
                    fetch.lsn_lo(),
                    fetch.lsn_hi(),
                )
                .await?;
                let payload = encode_repl_heal_response_frame(
                    frame.header().request_id(),
                    &ReplHealRespFrame::new(fetch.worker_id(), frames),
                )?;
                stream.write_all(&payload).await.map_err(|e| {
                    mudu_error!(ErrorCode::Network, "write repl heal response error", e)
                })?;
            }
            MessageType::ReplSnapshotFetch => {
                let fetch = decode_repl_snapshot_fetch_request(&frame)?;
                let payload =
                    serve_snapshot_fetch(server_instance_id, frame.header().request_id(), &fetch)
                        .await?;
                stream.write_all(&payload).await.map_err(|e| {
                    mudu_error!(ErrorCode::Network, "write repl snapshot response error", e)
                })?;
            }
            other => {
                let err = mudu_error!(
                    ErrorCode::Parse,
                    format!(
                        "unexpected message type {:?} on the replication channel",
                        other
                    )
                );
                let payload = encode_merror_response(frame.header().request_id(), &err)?;
                let _ = stream.write_all(&payload).await;
                return Err(err);
            }
        }
    }
    Ok(())
}

/// Subscriber resume points below this node's GC frontier are answered
/// with a snapshot offer instead of a stream.
async fn offer_snapshot(
    stream: &mut AsyncTcpStream,
    request_id: u64,
    handle: &crate::ReplWorkerHandle,
    worker_index: u32,
    from_lsn: u64,
) -> RS<bool> {
    let Some(log) = handle.worker().worker_log()? else {
        return Ok(false);
    };
    let frontier = log.layout()?.load_gc_frontier()?;
    let Some(frontier) = frontier else {
        return Ok(false);
    };
    if from_lsn >= frontier.lsn {
        return Ok(false);
    }
    let data_dir = handle.worker().storage().relation_path().to_string();
    let manifest = crate::snapshot::load_latest_manifest(&data_dir, worker_index)?;
    let Some(manifest) = manifest else {
        return Err(mudu_error!(
            ErrorCode::InvalidState,
            format!(
                "subscriber resumes at lsn {from_lsn} below the GC frontier {} but no snapshot is available",
                frontier.lsn
            )
        ));
    };
    // The offered snapshot must cover the frontier (GC never advances past
    // the latest snapshot point, by construction).
    let payload = encode_repl_snapshot_offer_frame(
        request_id,
        &ReplSnapshotOfferFrame::new(worker_index, manifest),
    )?;
    stream
        .write_all(&payload)
        .await
        .map_err(|e| mudu_error!(ErrorCode::Network, "write repl snapshot offer error", e))?;
    crate::note_snapshot_offer_served();
    trace!(
        worker_index,
        from_lsn,
        frontier_lsn = frontier.lsn,
        "replication subscribe answered with a snapshot offer"
    );
    Ok(true)
}

/// Serves one snapshot fetch page from this node's snapshot store.
async fn serve_snapshot_fetch(
    server_instance_id: ServerInstanceId,
    request_id: u64,
    fetch: &ReplSnapshotFetchReq,
) -> RS<Vec<u8>> {
    let handle = repl_worker(server_instance_id, fetch.worker_id())?;
    let data_dir = handle.worker().storage().relation_path().to_string();
    let dir =
        crate::snapshot::snapshot_version_dir(&data_dir, fetch.worker_id(), fetch.snapshot_lsn());
    let data = crate::snapshot::read_snapshot_page(&dir, fetch.path(), fetch.offset(), fetch.len())
        .await?;
    encode_repl_snapshot_resp_frame(
        request_id,
        &ReplSnapshotRespFrame::new(
            fetch.worker_id(),
            fetch.path().to_string(),
            fetch.offset(),
            data,
        ),
    )
}

/// Sends an error response on a failed subscription and closes cleanly.
async fn reject_subscription(
    stream: &mut AsyncTcpStream,
    request_id: u64,
    err: &mudu::error::MuduError,
    leader_hint: Option<LeaderHint>,
) -> RS<()> {
    let payload = match leader_hint {
        Some(hint) => encode_merror_response_with_leader_hint(request_id, err, hint)?,
        None => encode_merror_response(request_id, err)?,
    };
    stream.write_all(&payload).await.map_err(|e| {
        mudu_error!(
            ErrorCode::Network,
            "write repl subscribe error response error",
            e
        )
    })?;
    Ok(())
}

async fn run_subscription(
    server_instance_id: ServerInstanceId,
    mut stream: AsyncTcpStream,
    initial_buf: Vec<u8>,
    repl_cfg: ReplNodeCfg,
    request: ReplSubscribeReq,
    request_id: u64,
    stop_rx: StopRx,
) -> RS<()> {
    let Some((handle, log, own_epoch)) = validate_subscription(
        server_instance_id,
        &mut stream,
        &repl_cfg,
        &request,
        request_id,
    )
    .await?
    else {
        return Ok(());
    };
    let worker_index = request.worker_id();
    // The acceptance frame goes first: the subscriber verifies the
    // source's identity and learns its epoch.
    let accept = encode_repl_subscribe_accept_frame(
        request_id,
        &ReplSubscribeAcceptFrame::new(repl_cfg.node_id(), worker_index, own_epoch),
    )?;
    stream.write_all(&accept).await.map_err(|e| {
        mudu_error!(
            ErrorCode::Network,
            "write repl subscribe accept frame error",
            e
        )
    })?;
    // Resume points below the GC frontier are served the snapshot instead
    // of a stream (the frames are gone); the replica restores and then
    // resubscribes from the snapshot point.
    if offer_snapshot(
        &mut stream,
        request_id,
        &handle,
        worker_index,
        request.from_lsn(),
    )
    .await?
    {
        return Ok(());
    }
    register_repl_subscriber(server_instance_id, worker_index, request.src_node_id())?;
    let subscription_result = run_subscription_stream(
        server_instance_id,
        stream,
        initial_buf,
        repl_cfg,
        handle,
        log,
        request.src_node_id(),
        request.from_lsn(),
        stop_rx,
    )
    .await;
    unregister_repl_subscriber(server_instance_id, worker_index, request.src_node_id())?;
    subscription_result
}

/// Validates the subscription request against membership, worker
/// availability and the stream epoch, rejecting on the wire and returning
/// `None` on failure. On success returns the worker handle, its log and
/// this node's stream epoch.
///
/// Any node may serve a subscription: subscribers pick the current raft
/// leader themselves, and a stale stream is harmless (staging plus the
/// sink's byte-compare divergence heal converges it). Membership is
/// validated against the CURRENT committed conf in raft mode (online
/// membership changes), falling back to the static peer list in stream
/// mode.
async fn validate_subscription(
    server_instance_id: ServerInstanceId,
    stream: &mut AsyncTcpStream,
    repl_cfg: &ReplNodeCfg,
    request: &ReplSubscribeReq,
    request_id: u64,
) -> RS<Option<(crate::ReplWorkerHandle, ChunkedWorkerLogBackend, u64)>> {
    let subscriber_known = match crate::raft_share_for_node(repl_cfg.node_id()) {
        Some(share) => share
            .peers()
            .iter()
            .any(|peer| peer.node_id() == request.src_node_id()),
        None => repl_cfg
            .peers()
            .iter()
            .any(|peer| peer.node_id() == request.src_node_id()),
    };
    if !subscriber_known {
        let err = mudu_error!(
            ErrorCode::InvalidArgument,
            format!(
                "replication subscriber node {} is not a configured peer",
                request.src_node_id()
            )
        );
        reject_subscription(stream, request_id, &err, None).await?;
        return Ok(None);
    }
    let worker_index = request.worker_id();
    let handle = match repl_worker(server_instance_id, worker_index) {
        Ok(handle) => handle,
        Err(err) => {
            reject_subscription(stream, request_id, &err, None).await?;
            return Ok(None);
        }
    };
    let Some(log) = handle.worker().worker_log()? else {
        let err = mudu_error!(
            ErrorCode::Internal,
            "replication target worker has no worker log"
        );
        reject_subscription(stream, request_id, &err, None).await?;
        return Ok(None);
    };
    // A subscriber that knows a NEWER epoch than this node proves this node
    // is a stale (zombie) primary: refuse to serve it.
    let own_epoch = match handle.worker().repl_ctl()? {
        Some(ctl) => ctl.epoch(),
        None => 0,
    };
    if request.src_epoch() > own_epoch {
        let err = mudu_error!(
            ErrorCode::InvalidState,
            format!(
                "stale primary: subscriber knows epoch {}, this node is at epoch {}",
                request.src_epoch(),
                own_epoch
            )
        );
        reject_subscription(stream, request_id, &err, None).await?;
        return Ok(None);
    }
    Ok(Some((handle, log, own_epoch)))
}

/// The streaming phase of one accepted replication subscription.
// The parameters mirror the subscription context; a struct would only
// rename the same wiring.
#[allow(clippy::too_many_arguments)]
async fn run_subscription_stream(
    server_instance_id: ServerInstanceId,
    stream: AsyncTcpStream,
    initial_buf: Vec<u8>,
    repl_cfg: ReplNodeCfg,
    handle: crate::ReplWorkerHandle,
    log: ChunkedWorkerLogBackend,
    src_node_id: u64,
    from_lsn: u64,
    mut stop_rx: StopRx,
) -> RS<()> {
    let worker_index = handle
        .worker()
        .worker_index()
        .try_into()
        .map_err(|_| mudu_error!(ErrorCode::Parse, "worker index overflow"))?;
    trace!(
        worker_index,
        src_node_id,
        from_lsn,
        "replication subscription started"
    );

    let (reader, mut writer) = tokio::io::split(stream);
    let flow = Arc::new(ReplFlowWindow::new(
        repl_cfg.repl_flow_window_segments().max(1),
    ));
    let (out_tx, mut out_rx) = unbounded_channel::<Vec<u8>>();
    let (conn_stop_tx, conn_stop_rx) = stop_channel();

    // Reader half: acks advance the flow window and are recorded; heal
    // fetches are answered through the outgoing queue the writer drains.
    let reader_ctx = StreamReaderCtx {
        server_instance_id,
        worker_index,
        src_node_id,
        flow: flow.clone(),
        out_tx,
        worker: handle.worker().clone(),
        state: handle.state().clone(),
    };
    let reader_join = spawn_local_detached("repl_stream_reader", async move {
        reader_ctx.run(reader, initial_buf, conn_stop_rx).await
    })?;

    // Writer half: stream WAL frames, bounded by the flow window. Each
    // batch carries this node's current stream epoch (0 in raft mode);
    // it is read per batch because a promote advances it at runtime.
    let writer_worker = handle.worker().clone();
    let mut cursor = ChunkCursor::new(log, from_lsn);
    let write_result = run_stream_writer(
        &mut writer,
        &mut out_rx,
        &flow,
        &mut stop_rx,
        &writer_worker,
        &mut cursor,
        worker_index,
    )
    .await;
    conn_stop_tx.stop();
    let _ = writer.shutdown().await;
    match reader_join.await {
        Ok(Some(reader_result)) => reader_result?,
        Ok(None) => {}
        Err(err) => {
            return Err(mudu_error!(
                ErrorCode::Tokio,
                "join repl stream reader error",
                err
            ))
        }
    }
    write_result
}

/// Context of the reader half of one subscription stream.
struct StreamReaderCtx {
    server_instance_id: ServerInstanceId,
    worker_index: u32,
    src_node_id: u64,
    flow: Arc<ReplFlowWindow>,
    out_tx: mudu_sys::tokio::sync::mpsc::UnboundedSender<Vec<u8>>,
    worker: Arc<WorkerRuntime>,
    state: Arc<ReplWorkerState>,
}

impl StreamReaderCtx {
    /// Runs the reader half until the connection closes or the writer half
    /// stops it: acks advance the flow window and are recorded; heal
    /// fetches are answered through the outgoing queue the writer drains.
    async fn run(
        self,
        mut reader: tokio::io::ReadHalf<AsyncTcpStream>,
        initial_buf: Vec<u8>,
        mut conn_stop_rx: StopRx,
    ) -> RS<()> {
        let mut read_buf = initial_buf;
        let mut chunk = vec![0u8; 8192];
        while let Some(frame) =
            read_next_frame(&mut reader, &mut read_buf, &mut chunk, &mut conn_stop_rx).await?
        {
            match frame.header().message_type() {
                MessageType::ReplAck => {
                    let ack = decode_repl_ack_frame(&frame)?;
                    if ack.worker_id() == self.worker_index {
                        record_repl_ack(
                            self.server_instance_id,
                            self.worker_index,
                            self.src_node_id,
                            ack.stored_lsn(),
                        )?;
                        self.flow.ack_one();
                    }
                }
                MessageType::ReplHealFetch => {
                    let fetch = decode_repl_heal_fetch_request(&frame)?;
                    if fetch.worker_id() != self.worker_index {
                        continue;
                    }
                    let frames = gather_frames_for_heal(
                        &self.worker,
                        &self.state,
                        fetch.lsn_lo(),
                        fetch.lsn_hi(),
                    )
                    .await?;
                    let payload = encode_repl_heal_response_frame(
                        frame.header().request_id(),
                        &ReplHealRespFrame::new(self.worker_index, frames),
                    )?;
                    self.out_tx.send(payload).map_err(|_| {
                        mudu_error!(ErrorCode::ChannelClosed, "repl outgoing queue closed")
                    })?;
                }
                other => {
                    return Err(mudu_error!(
                        ErrorCode::Parse,
                        format!("unexpected replication stream frame {:?}", other)
                    ));
                }
            }
        }
        Ok(())
    }
}

/// The writer half of one subscription: drains queued heal responses, then
/// streams WAL frames bounded by the flow window. Each batch carries this
/// node's current stream epoch (0 in raft mode); it is read per batch
/// because a promote advances it at runtime.
async fn run_stream_writer(
    writer: &mut tokio::io::WriteHalf<AsyncTcpStream>,
    out_rx: &mut mudu_sys::tokio::sync::mpsc::UnboundedReceiver<Vec<u8>>,
    flow: &Arc<ReplFlowWindow>,
    stop_rx: &mut StopRx,
    worker: &Arc<WorkerRuntime>,
    cursor: &mut ChunkCursor,
    worker_index: u32,
) -> RS<()> {
    let mut write_request_id = 1u64;
    'write_loop: loop {
        if stop_rx.is_stopped() {
            break Ok(());
        }
        while let Ok(bytes) = out_rx.try_recv() {
            if let Err(err) = writer.write_all(&bytes).await {
                break 'write_loop Err(mudu_error!(
                    ErrorCode::Network,
                    "write repl heal response error",
                    err
                ));
            }
        }
        if flow.is_full() {
            tokio::select! {
                _ = flow.notified() => {}
                _ = mudu_sys::task::async_::sleep(REPL_POLL_INTERVAL) => {}
                _ = stop_rx.changed() => {}
            }
            continue;
        }
        let batch_epoch = worker.repl_ctl()?.map(|ctl| ctl.epoch()).unwrap_or(0);
        match cursor.next_batch().await {
            Ok(Some((lsn_lo, frames))) => {
                let payload = match encode_repl_batch_frame(
                    write_request_id,
                    &ReplBatchFrame::new_with_epoch(worker_index, lsn_lo, frames, batch_epoch),
                ) {
                    Ok(payload) => payload,
                    Err(err) => break Err(err),
                };
                write_request_id += 1;
                if let Err(err) = writer.write_all(&payload).await {
                    break Err(mudu_error!(
                        ErrorCode::Network,
                        "write repl batch error",
                        err
                    ));
                }
                flow.sent_one();
            }
            Ok(None) => {
                tokio::select! {
                    _ = flow.notified() => {}
                    _ = mudu_sys::task::async_::sleep(REPL_POLL_INTERVAL) => {}
                    _ = stop_rx.changed() => {}
                }
            }
            Err(err) => break Err(err),
        }
    }
}

/// Shared flow-control state of one subscription: the number of batches sent
/// but not yet acked, capped at the configured window.
struct ReplFlowWindow {
    in_flight: AtomicU64,
    limit: u64,
    notify: Notify,
}

impl ReplFlowWindow {
    fn new(limit: u64) -> Self {
        Self {
            in_flight: AtomicU64::new(0),
            limit,
            notify: Notify::new(),
        }
    }

    fn is_full(&self) -> bool {
        self.in_flight.load(Ordering::Relaxed) >= self.limit
    }

    fn sent_one(&self) {
        self.in_flight.fetch_add(1, Ordering::Relaxed);
    }

    fn ack_one(&self) {
        let mut current = self.in_flight.load(Ordering::Relaxed);
        while current > 0 {
            match self.in_flight.compare_exchange_weak(
                current,
                current - 1,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(observed) => current = observed,
            }
        }
        self.notify.notify_one();
    }

    async fn notified(&self) {
        self.notify.notified().await;
    }
}

/// Cursor over the chunk files of one worker log, tracking the file offset
/// up to which frames were already streamed.
struct ChunkCursor {
    log: ChunkedWorkerLogBackend,
    from_lsn: u64,
    current_path: Option<PathBuf>,
    offset: u64,
    open_file: Option<(PathBuf, Arc<dyn AsyncFile>)>,
}

impl ChunkCursor {
    fn new(log: ChunkedWorkerLogBackend, from_lsn: u64) -> Self {
        Self {
            log,
            from_lsn,
            current_path: None,
            offset: 0,
            open_file: None,
        }
    }

    /// Collects the next batch of frames with `lsn >= from_lsn`. Returns
    /// `None` when the cursor caught up with the current end of the log.
    async fn next_batch(&mut self) -> RS<Option<(u64, Vec<Vec<u8>>)>> {
        loop {
            // Re-list every round: the flush driver may have rotated to a new
            // chunk since the previous poll.
            let chunks = self.log.chunk_paths_sorted().await?;
            let Some(chunk_idx) = self.current_chunk_index(&chunks)? else {
                return Ok(None);
            };
            let path = chunks[chunk_idx].clone();
            let file = self.open_current(&path).await?;
            let file_len = file.file_len().await?;
            if self.offset >= file_len {
                // Current chunk exhausted. Only a later chunk can hold more
                // frames; when this is the last chunk the stream is caught up.
                if chunk_idx + 1 < chunks.len() {
                    self.current_path = Some(chunks[chunk_idx + 1].clone());
                    self.offset = 0;
                    self.open_file = None;
                    continue;
                }
                return Ok(None);
            }
            let mut read_len = (file_len - self.offset).min(REPL_READ_SLICE_CAP);
            let mut bytes = file.read_exact_at(self.offset, read_len as usize).await?;
            // A single frame may exceed the slice cap; grow the window until
            // one whole frame fits or the chunk end is reached.
            while scan_valid_frame_prefix(&bytes).frames.is_empty()
                && read_len < file_len - self.offset
            {
                read_len = (read_len.saturating_mul(2)).min(file_len - self.offset);
                bytes = file.read_exact_at(self.offset, read_len as usize).await?;
            }
            let prefix = scan_valid_frame_prefix(&bytes);
            if prefix.frames.is_empty() {
                // Torn tail: the flush driver may complete it later when this
                // is the last chunk; earlier chunks are complete, so skip.
                if chunk_idx + 1 < chunks.len() {
                    self.current_path = Some(chunks[chunk_idx + 1].clone());
                    self.offset = 0;
                    self.open_file = None;
                    continue;
                }
                return Ok(None);
            }
            self.offset += prefix.valid_len as u64;
            let mut lsn_lo = None;
            let mut frames = Vec::new();
            for frame in prefix.frames {
                let lsn = frame_lsn(&frame)?.as_u64();
                if lsn < self.from_lsn {
                    continue;
                }
                if lsn_lo.is_none() {
                    lsn_lo = Some(lsn);
                }
                frames.push(frame);
            }
            if frames.is_empty() {
                continue;
            }
            let Some(lsn_lo) = lsn_lo else {
                continue;
            };
            return Ok(Some((lsn_lo, frames)));
        }
    }

    fn current_chunk_index(&self, chunks: &[PathBuf]) -> RS<Option<usize>> {
        match &self.current_path {
            None => Ok(if chunks.is_empty() { None } else { Some(0) }),
            Some(path) => {
                let index = chunks.iter().position(|candidate| candidate == path);
                if index.is_none() {
                    return Err(mudu_error!(
                        ErrorCode::EntityNotFound,
                        format!("replicated worker log chunk {} disappeared", path.display())
                    ));
                }
                Ok(index)
            }
        }
    }

    async fn open_current(&mut self, path: &Path) -> RS<Arc<dyn AsyncFile>> {
        if let Some((open_path, file)) = &self.open_file {
            if open_path == path {
                return Ok(file.clone());
            }
        }
        let file = self.log.fs().open(path, FileOptions::read_only()).await?;
        self.open_file = Some((path.to_path_buf(), file.clone()));
        Ok(file)
    }
}

/// Collects all frames with `lsn_lo <= lsn < lsn_hi` across the worker log's
/// chunks, in log order.
async fn gather_frames_in_range(
    log: &ChunkedWorkerLogBackend,
    lsn_lo: u64,
    lsn_hi: u64,
) -> RS<Vec<Vec<u8>>> {
    let fs = log.fs();
    let mut frames = Vec::new();
    for path in log.chunk_paths_sorted().await? {
        let bytes = fs.read_all(&path).await?;
        if bytes.is_empty() {
            continue;
        }
        let prefix = scan_valid_frame_prefix(&bytes);
        for frame in prefix.frames {
            let lsn = frame_lsn(&frame)?.as_u64();
            if lsn >= lsn_hi {
                return Ok(frames);
            }
            if lsn >= lsn_lo {
                frames.push(frame);
            }
        }
    }
    Ok(frames)
}

/// Collects the frames `[lsn_lo, lsn_hi)` a voter can serve for healing:
/// the union of the worker WAL and the staging store, in LSN order, with
/// WAL content winning on overlap (the promoted prefix is the
/// committed-trusted one).
async fn gather_frames_for_heal(
    worker: &Arc<WorkerRuntime>,
    state: &Arc<ReplWorkerState>,
    lsn_lo: u64,
    lsn_hi: u64,
) -> RS<Vec<Vec<u8>>> {
    let Some(log) = worker.worker_log()? else {
        return Err(mudu_error!(
            ErrorCode::Internal,
            "replication target worker has no worker log"
        ));
    };
    let mut by_lsn: BTreeMap<u64, Vec<u8>> = BTreeMap::new();
    if let Some(staging) = state.staging()? {
        for frame in staging.frames_in_range(lsn_lo, lsn_hi).await? {
            let lsn = frame_lsn(&frame)?.as_u64();
            by_lsn.insert(lsn, frame);
        }
    }
    for frame in gather_frames_in_range(&log, lsn_lo, lsn_hi).await? {
        let lsn = frame_lsn(&frame)?.as_u64();
        by_lsn.insert(lsn, frame);
    }
    Ok(by_lsn.into_values().collect())
}
