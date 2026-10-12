//! Follower-side replication sink, one task per worker.
//!
//! Raft mode (`[replication.raft]`): the sink subscribes to the CURRENT
//! RAFT LEADER's replication listener and stages the streamed WAL frames
//! into the worker's staging store (design invariant I2: nothing reaches
//! the worker WAL before it is raft-committed; the applier promotes).
//! Acknowledgements report the staged-and-flushed LSN, which is exactly
//! what the leader's commit sequencer waits on for its majority gate.
//!
//! Stream mode (`[replication.stream]`): the sink subscribes to the
//! CONFIGURED PRIMARY, appends the streamed frames to the worker WAL
//! directly, replays them into storage immediately (no raft, no committed
//! vector) and acks the stored LSN — which is what the synchronous primary
//! waits on when `sync = true`.
//!
//! Divergence heal (raft mode): the subscription always resumes at the
//! applier's chain point. Frames the sink already staged at those LSNs are
//! byte-compared (CRC64): identical frames are skipped; the first mismatch
//! truncates the speculative staged tail from that LSN and the leader's
//! content replaces it. Committed points can never conflict because they
//! come from raft.

use crate::ctl::ReplModeCtl;
use crate::raft_service::RaftShare;
use crate::staging::{staged_frame_lsn, StagingStore};
use crate::{read_next_frame, scan_worker_log_tail, ApplierMsg, ReplWorkerState};
use ::wal::lsn::LSN;
use ::wal::worker_log::{ChunkedWorkerLogBackend, WorkerLogBackend};
use ::wal::xl_batch::decode_xl_batches_with_pending;
use ::worker::worker::WorkerRuntime;
use mudu::common::crc::crc64;
use mudu::common::result::RS;
use mudu::error::{ErrorCode, MuduError};
use mudu::mudu_error;
use mudu_contract::protocol::{
    decode_error_response, decode_repl_batch_frame, decode_repl_snapshot_offer_frame,
    decode_repl_subscribe_accept_frame, encode_repl_ack_frame, encode_repl_subscribe_request,
    ErrorResponse, Frame, MessageType, ReplAckFrame, ReplBatchFrame, ReplSnapshotManifest,
    ReplSubscribeReq,
};
use mudu_sys::net::AsyncTcpStream;
use mudu_sys::sync::async_::stop_flag::StopRx;
use mudu_sys::tokio;
use mudu_sys::tokio::io::AsyncWriteExt;
use std::sync::Arc;
use std::time::Duration;
use tracing::{trace, warn};

/// Backoff between a failed/closed replication stream and the resubscribe.
const REPL_RECONNECT_BACKOFF: Duration = Duration::from_millis(500);

/// How often the sink re-checks raft leadership while it has no stream
/// target (self is leader, no leader known, or the sink is paused).
const REPL_IDLE_POLL: Duration = Duration::from_millis(100);

/// Budget of one applier-side snapshot install round-trip.
const RESTORE_APPLY_TIMEOUT: Duration = Duration::from_secs(120);

/// What the sink does with received frames and how it picks its stream
/// target.
pub enum SinkDriver {
    /// Raft mode: stage into the staging store (the applier promotes
    /// raft-committed frames), targeting the current raft leader.
    Raft {
        /// The raft share: leader discovery and the sink-pause flag.
        share: Arc<RaftShare>,
        /// The staging store received frames land in.
        staging: Arc<StagingStore>,
        /// The worker's shared replication state (chain point, applier
        /// channel).
        state: Arc<ReplWorkerState>,
    },
    /// Stream mode: append to the worker WAL and replay immediately,
    /// targeting the current primary (read from the shared control).
    Stream {
        /// The shared stream-mode control (current primary and epoch).
        ctl: ReplModeCtl,
        /// This node's cluster id, sent in the subscribe request.
        node_id: u64,
        /// The worker log received frames are appended to.
        log: ChunkedWorkerLogBackend,
        /// Applied-LSN growth that triggers a local snapshot.
        snapshot_threshold_bytes: u64,
    },
}

/// Why one stream round ended.
enum StreamOutcome {
    /// The connection closed or errored; the sink reconnects.
    Interrupted,
    /// The source answered with a snapshot offer (our resume point fell
    /// below its GC frontier); the sink restores before resubscribing.
    Offered(ReplSnapshotManifest),
}

/// Where one stream round goes and from where it resumes.
struct StreamTarget {
    addr: String,
    from_lsn: u64,
    next_expected: u64,
}

/// Runs the replication sink of one worker until `stop_rx` fires.
pub async fn run_repl_sink(
    worker: WorkerRuntime,
    driver: SinkDriver,
    mut stop_rx: StopRx,
) -> RS<()> {
    let worker_index = u32::try_from(worker.worker_index()).map_err(|_| {
        mudu_error!(
            ErrorCode::Parse,
            format!(
                "worker index {} does not fit the replication protocol",
                worker.worker_index()
            )
        )
    })?;
    trace!(worker_index, "replication sink starting");
    let mut direct_state = init_direct_state(&driver).await?;
    // The latest snapshot point this worker reached (local or restored);
    // stream replicas snapshot when the resume point crosses it by the
    // configured threshold.
    let mut last_snapshot_lsn =
        crate::snapshot::load_latest_manifest(worker.storage().relation_path(), worker_index)?
            .map(|manifest| manifest.committed_lsn)
            .unwrap_or(0);
    // Stream mode: the last subscription target, to detect a follow change
    // and trigger the overlap probe (CRC byte-compare of the tail window).
    let mut last_target_addr: Option<String> = None;
    while !stop_rx.is_stopped() {
        let Some(target) = select_target(&driver, direct_state.as_ref())? else {
            idle_wait(&mut stop_rx).await;
            continue;
        };
        note_target_change(
            &mut direct_state,
            &mut last_target_addr,
            last_snapshot_lsn,
            &target.addr,
        );
        let stream_result = stream_from_leader(
            &worker,
            &driver,
            worker_index,
            &target,
            &mut direct_state,
            &mut last_snapshot_lsn,
            stop_rx.clone(),
        )
        .await;
        handle_stream_outcome(
            &worker,
            &driver,
            worker_index,
            &target,
            stream_result,
            &mut direct_state,
            &mut last_snapshot_lsn,
        )
        .await;
        if reconnect_backoff_stopped(&stop_rx).await {
            break;
        }
    }
    trace!(worker_index, "replication sink stopped");
    Ok(())
}

/// Stream mode: resume state derived once from the local WAL and carried
/// across reconnects (raft mode stages instead and carries none).
async fn init_direct_state(driver: &SinkDriver) -> RS<Option<DirectResumeState>> {
    match driver {
        SinkDriver::Stream { log, ctl, .. } => {
            let (resume, pending_frames, pending_start_lsn) = scan_worker_log_tail(log).await?;
            Ok(Some(DirectResumeState {
                resume,
                pending_frames,
                pending_start_lsn,
                probe_from: None,
                local_crcs: std::collections::BTreeMap::new(),
                known_epoch: ctl.epoch(),
            }))
        }
        SinkDriver::Raft { .. } => Ok(None),
    }
}

/// Stream mode: on a follow target change, arms the overlap probe from the
/// last trusted point (latest snapshot, else the log start) so a divergent
/// local tail is truncated by CRC byte-compare.
fn note_target_change(
    direct_state: &mut Option<DirectResumeState>,
    last_target_addr: &mut Option<String>,
    last_snapshot_lsn: u64,
    target_addr: &str,
) {
    if let Some(state) = direct_state.as_mut() {
        let changed = last_target_addr
            .as_ref()
            .is_some_and(|last| *last != target_addr);
        if changed {
            state.probe_from = Some(last_snapshot_lsn.saturating_add(1).min(state.resume));
            state.local_crcs.clear();
        }
    }
    *last_target_addr = Some(target_addr.to_string());
}

/// Handles one ended stream round: an offered snapshot is restored inline.
/// Failures only log; the next round resubscribes.
async fn handle_stream_outcome(
    worker: &WorkerRuntime,
    driver: &SinkDriver,
    worker_index: u32,
    target: &StreamTarget,
    stream_result: RS<StreamOutcome>,
    direct_state: &mut Option<DirectResumeState>,
    last_snapshot_lsn: &mut u64,
) {
    match stream_result {
        Ok(StreamOutcome::Interrupted) => {}
        Ok(StreamOutcome::Offered(manifest)) => {
            if let Err(err) = restore_from_offer(
                worker,
                driver,
                worker_index,
                &target.addr,
                manifest,
                direct_state,
                last_snapshot_lsn,
            )
            .await
            {
                warn!(
                    worker_index,
                    "snapshot restore failed, resubscribing: {}", err
                );
            }
        }
        Err(err) => {
            warn!(
                worker_index,
                "replication stream interrupted, resubscribing: {}", err
            );
        }
    }
}

/// Sleeps out the reconnect backoff; returns `true` when the sink was
/// stopped (before or during the backoff) and must exit.
async fn reconnect_backoff_stopped(stop_rx: &StopRx) -> bool {
    if stop_rx.is_stopped() {
        return true;
    }
    let mut backoff_stop_rx = stop_rx.clone();
    tokio::select! {
        _ = mudu_sys::task::async_::sleep(REPL_RECONNECT_BACKOFF) => {}
        _ = backoff_stop_rx.changed() => {}
    }
    stop_rx.is_stopped()
}

/// Downloads the offered snapshot and installs it: raft mode delegates the
/// state mutation to the applier (its serial point), stream mode restores
/// inline in this sink (its own serial point).
async fn restore_from_offer(
    worker: &WorkerRuntime,
    driver: &SinkDriver,
    worker_index: u32,
    addr: &str,
    manifest: ReplSnapshotManifest,
    direct_state: &mut Option<DirectResumeState>,
    last_snapshot_lsn: &mut u64,
) -> RS<()> {
    let data_dir = worker.storage().relation_path().to_string();
    let version_dir =
        crate::snapshot::snapshot_version_dir(&data_dir, worker_index, manifest.committed_lsn);
    crate::snapshot::fetch_snapshot(addr, worker_index, &manifest, &version_dir).await?;
    let resume = manifest.resume_lsn();
    let committed_lsn = manifest.committed_lsn;
    match driver {
        SinkDriver::Raft { state, .. } => {
            let tx = state.applier_tx()?.ok_or_else(|| {
                mudu_error!(
                    ErrorCode::InvalidState,
                    "snapshot restore: applier not running"
                )
            })?;
            let (respond_tx, respond_rx) = mudu_sys::tokio::sync::oneshot::channel();
            tx.send(ApplierMsg::InstallSnapshot {
                manifest,
                snapshot_dir: version_dir,
                respond: respond_tx,
            })
            .map_err(|_| mudu_error!(ErrorCode::ChannelClosed, "applier channel closed"))?;
            match mudu_sys::timeout(RESTORE_APPLY_TIMEOUT, respond_rx).await {
                Some(Ok(result)) => result?,
                Some(Err(_closed)) => {
                    return Err(mudu_error!(
                        ErrorCode::ChannelClosed,
                        "applier dropped the snapshot install"
                    ));
                }
                None => {
                    return Err(mudu_error!(
                        ErrorCode::TimedOut,
                        "snapshot install timed out at the applier"
                    ));
                }
            }
        }
        SinkDriver::Stream { log, .. } => {
            // Drain old dirty pages before the files are replaced, then
            // reload catalog + relations + kv from the snapshot and reclaim
            // the WAL it covers.
            ::worker::worker::flush_worker_dirty_pages(worker).await?;
            crate::snapshot::install_snapshot_relation_files(&data_dir, &version_dir, &manifest)
                .await?;
            worker.meta_mgr().initialize().await?;
            worker.storage().reload_relations_from_disk().await?;
            let kv = crate::snapshot::read_kv_dump(&version_dir)?;
            worker.storage().restore_kv_latest(kv).await?;
            if log.next_lsn().as_u64() > resume {
                log.truncate_from_lsn(LSN::new(resume)).await?;
            }
            let _ = log
                .layout()?
                .gc_prefix_chunks(log.fs().as_ref(), resume)
                .await?;
            if let Some(state) = direct_state.as_mut() {
                state.resume = resume;
                state.pending_frames.clear();
                state.pending_start_lsn = None;
            }
        }
    }
    *last_snapshot_lsn = committed_lsn.max(*last_snapshot_lsn);
    crate::note_snapshot_restore_completed();
    trace!(
        worker_index,
        committed_lsn,
        "snapshot restored from the replication source"
    );
    Ok(())
}

/// The sink's resume state in replication mode: the next LSN needed from
/// the primary plus the trailing not-yet-terminated entry frames kept for
/// XL reassembly across batches and reconnects. `probe_from`/`local_crcs`
/// drive the overlap divergence check after a follow target change;
/// `known_epoch` is the highest stream epoch accepted so far.
struct DirectResumeState {
    resume: u64,
    pending_frames: Vec<Vec<u8>>,
    pending_start_lsn: Option<LSN>,
    probe_from: Option<u64>,
    local_crcs: std::collections::BTreeMap<u64, u64>,
    known_epoch: u64,
}

/// Picks the stream target for one round: the current raft leader (raft
/// mode) or the configured primary (replication mode). `None` while this
/// node is the primary/leader, no leader is known, or the sink is paused.
fn select_target(
    driver: &SinkDriver,
    direct_state: Option<&DirectResumeState>,
) -> RS<Option<StreamTarget>> {
    match driver {
        SinkDriver::Raft {
            share,
            staging,
            state,
        } => {
            if share.sink_paused() {
                return Ok(None);
            }
            if share.role() == mudu_raft::RaftRole::Leader {
                return Ok(None);
            }
            let Some(addr) = share.leader_id().and_then(|leader| {
                if leader == share.node_id() {
                    None
                } else {
                    share.peer_addr(leader)
                }
            }) else {
                return Ok(None);
            };
            // The chain cell is the resume point: the next LSN to commit.
            let from_lsn = state.chain()?.0;
            let staged_next = staging
                .max_lsn()?
                .map(|lsn| lsn.as_u64().saturating_add(1))
                .unwrap_or(0);
            let next_expected = staged_next.max(from_lsn);
            Ok(Some(StreamTarget {
                addr,
                from_lsn,
                next_expected,
            }))
        }
        SinkDriver::Stream { ctl, .. } => {
            if ctl.is_primary() {
                return Ok(None);
            }
            let Some(addr) = ctl.primary_addr() else {
                return Ok(None);
            };
            let Some(state) = direct_state else {
                return Ok(None);
            };
            Ok(Some(StreamTarget {
                addr,
                from_lsn: state.probe_from.unwrap_or(state.resume),
                next_expected: state.resume,
            }))
        }
    }
}

async fn idle_wait(stop_rx: &mut StopRx) {
    tokio::select! {
        _ = mudu_sys::task::async_::sleep(REPL_IDLE_POLL) => {}
        _ = stop_rx.changed() => {}
    }
}

/// An open replication stream to the source with its read buffers and the
/// next request id.
struct ReplConn {
    stream: AsyncTcpStream,
    read_buf: Vec<u8>,
    chunk: Vec<u8>,
    next_request_id: u64,
}

/// Connects to the stream target, sends the subscribe request from the
/// target's resume point and verifies the source's acceptance frame.
async fn connect_and_subscribe(
    worker: &WorkerRuntime,
    driver: &SinkDriver,
    worker_index: u32,
    target: &StreamTarget,
    direct_state: &mut Option<DirectResumeState>,
    stop_rx: &mut StopRx,
) -> RS<ReplConn> {
    let addr = &target.addr;
    let stream = AsyncTcpStream::connect(addr.as_str()).await.map_err(|e| {
        mudu_error!(
            ErrorCode::Network,
            format!("connect replication leader {} error", addr),
            e
        )
    })?;
    stream
        .set_nodelay(true)
        .map_err(|e| mudu_error!(ErrorCode::Network, "set repl stream nodelay error", e))?;
    let mut conn = ReplConn {
        stream,
        read_buf: Vec::with_capacity(8192),
        chunk: vec![0u8; 8192],
        next_request_id: 1,
    };
    let (self_node_id, src_epoch) = match driver {
        SinkDriver::Raft { share, .. } => (share.node_id(), 0),
        SinkDriver::Stream { node_id, ctl, .. } => (*node_id, ctl.epoch()),
    };
    let subscribe = encode_repl_subscribe_request(
        conn.next_request_id,
        &ReplSubscribeReq::new_with_epoch(self_node_id, worker_index, target.from_lsn, src_epoch),
    )?;
    conn.next_request_id += 1;
    conn.stream
        .write_all(&subscribe)
        .await
        .map_err(|e| mudu_error!(ErrorCode::Network, "write repl subscribe request error", e))?;
    // The source's acceptance frame comes first.
    let first = read_next_frame(
        &mut conn.stream,
        &mut conn.read_buf,
        &mut conn.chunk,
        stop_rx,
    )
    .await?
    .ok_or_else(|| {
        mudu_error!(
            ErrorCode::Network,
            "replication source closed before the acceptance frame"
        )
    })?;
    verify_acceptance(&first, worker, driver, addr, direct_state).await?;
    Ok(conn)
}

/// Verifies the source's acceptance frame: a follow must land on the
/// intended primary (identity check), and a zombie primary's lower-epoch
/// stream is rejected (epoch check, recording a newer epoch).
async fn verify_acceptance(
    first: &Frame,
    worker: &WorkerRuntime,
    driver: &SinkDriver,
    addr: &str,
    direct_state: &mut Option<DirectResumeState>,
) -> RS<()> {
    match first.header().message_type() {
        MessageType::ReplSubscribeAccept => {
            let accept = decode_repl_subscribe_accept_frame(first)?;
            if let SinkDriver::Stream { ctl, .. } = driver {
                if let Some(expected) = ctl.primary_node_id() {
                    if accept.node_id() != expected {
                        return Err(mudu_error!(
                            ErrorCode::InvalidState,
                            format!(
                                "primary identity mismatch at {addr}: expected node {expected}, found node {}",
                                accept.node_id()
                            )
                        ));
                    }
                }
                if let Some(state) = direct_state.as_mut() {
                    if accept.epoch() < state.known_epoch {
                        return Err(mudu_error!(
                            ErrorCode::InvalidState,
                            format!(
                                "stale primary at {addr}: epoch {} below the known epoch {}",
                                accept.epoch(),
                                state.known_epoch
                            )
                        ));
                    }
                    note_stream_epoch(worker, ctl, state, accept.epoch()).await?;
                }
            }
            Ok(())
        }
        MessageType::Error => {
            let error = decode_error_response(first)?;
            Err(repl_remote_error(&error))
        }
        other => Err(mudu_error!(
            ErrorCode::Parse,
            format!("expected a subscribe acceptance frame, got {other:?}")
        )),
    }
}

/// One subscription round: connect, subscribe from the resume point and
/// process batches until the connection breaks, the sink pauses, the
/// source offers a snapshot, or the sink is stopped. `next_expected` is
/// the next LSN the sink does not yet hold.
async fn stream_from_leader(
    worker: &WorkerRuntime,
    driver: &SinkDriver,
    worker_index: u32,
    target: &StreamTarget,
    direct_state: &mut Option<DirectResumeState>,
    last_snapshot_lsn: &mut u64,
    mut stop_rx: StopRx,
) -> RS<StreamOutcome> {
    let mut conn = connect_and_subscribe(
        worker,
        driver,
        worker_index,
        target,
        direct_state,
        &mut stop_rx,
    )
    .await?;
    trace!(
        worker_index,
        from_lsn = target.from_lsn,
        next_expected = target.next_expected,
        "replication subscribed"
    );
    let mut round = StreamRound {
        worker,
        driver,
        worker_index,
        addr: &target.addr,
        direct_state,
        last_snapshot_lsn,
        staged: StagingTrack {
            next_expected: target.next_expected,
            staged_hi: match driver {
                SinkDriver::Raft { staging, .. } => staging.max_lsn()?.map(|lsn| lsn.as_u64()),
                SinkDriver::Stream { .. } => None,
            },
        },
    };
    while let Some(frame) = read_next_frame(
        &mut conn.stream,
        &mut conn.read_buf,
        &mut conn.chunk,
        &mut stop_rx,
    )
    .await?
    {
        if let SinkDriver::Raft { share, .. } = driver {
            if share.sink_paused() {
                return Ok(StreamOutcome::Interrupted);
            }
        }
        match frame.header().message_type() {
            MessageType::ReplBatch => {
                // Stream mode: drop the connection when the control flipped
                // (this node was promoted) or the primary changed (follow).
                if let SinkDriver::Stream { ctl, .. } = driver {
                    if ctl.is_primary()
                        || ctl.primary_addr().as_deref() != Some(target.addr.as_str())
                    {
                        return Ok(StreamOutcome::Interrupted);
                    }
                }
                let Some(stored_lsn) = round.store_batch(&frame).await? else {
                    continue;
                };
                round.write_ack(&mut conn, stored_lsn).await?;
            }
            MessageType::ReplHealResp => {
                // The sink never issues heal fetches; ignore stray responses.
                trace!(
                    worker_index,
                    "ignoring unexpected replication heal response"
                );
            }
            MessageType::ReplSnapshotOffer => return decode_snapshot_offer(&frame, worker_index),
            MessageType::Error => {
                let error = decode_error_response(&frame)?;
                return Err(repl_remote_error(&error));
            }
            other => {
                return Err(mudu_error!(
                    ErrorCode::Parse,
                    format!("unexpected replication stream frame {:?}", other)
                ));
            }
        }
    }
    Ok(StreamOutcome::Interrupted)
}

/// Decodes a snapshot offer for this worker into the round outcome.
fn decode_snapshot_offer(frame: &Frame, worker_index: u32) -> RS<StreamOutcome> {
    let offer = decode_repl_snapshot_offer_frame(frame)?;
    if offer.worker_id() != worker_index {
        return Err(mudu_error!(
            ErrorCode::Parse,
            format!(
                "snapshot offer for worker {} arrived on worker {} sink",
                offer.worker_id(),
                worker_index
            )
        ));
    }
    Ok(StreamOutcome::Offered(offer.into_manifest()))
}

/// The mutable per-round state of one subscription stream, threaded
/// through the batch handlers.
struct StreamRound<'a> {
    worker: &'a WorkerRuntime,
    driver: &'a SinkDriver,
    worker_index: u32,
    addr: &'a str,
    direct_state: &'a mut Option<DirectResumeState>,
    last_snapshot_lsn: &'a mut u64,
    staged: StagingTrack,
}

impl StreamRound<'_> {
    /// Decodes one ReplBatch frame and stores it (raft: stage; stream:
    /// direct apply), returning the stored-LSN watermark to ack; `None`
    /// skips the ack (nothing durably staged yet).
    async fn store_batch(&mut self, frame: &Frame) -> RS<Option<u64>> {
        let batch = decode_repl_batch_frame(frame)?;
        if batch.worker_id() != self.worker_index {
            return Err(mudu_error!(
                ErrorCode::Parse,
                format!(
                    "replication batch for worker {} arrived on worker {} sink",
                    batch.worker_id(),
                    self.worker_index
                )
            ));
        }
        let driver = self.driver;
        match driver {
            SinkDriver::Raft { staging, state, .. } => {
                stage_frames(staging, batch.into_frames(), &mut self.staged).await?;
                staging.flush().await?;
                state.notify_staging();
                // Ack only what is durably staged: the leader's
                // majority gate reads this as "stored", so an ack
                // must never claim content the follower lacks.
                let Some(stored_lsn) = self.staged.staged_hi else {
                    trace!(
                        worker_index = self.worker_index,
                        "skipping repl ack: nothing staged yet (unexpected)"
                    );
                    return Ok(None);
                };
                Ok(Some(stored_lsn))
            }
            SinkDriver::Stream {
                ctl,
                log,
                snapshot_threshold_bytes,
                ..
            } => self
                .apply_stream_batch(ctl, log, *snapshot_threshold_bytes, batch)
                .await
                .map(Some),
        }
    }

    /// Writes the stored-LSN ack of one processed batch.
    async fn write_ack(&self, conn: &mut ReplConn, stored_lsn: u64) -> RS<()> {
        let ack = encode_repl_ack_frame(
            conn.next_request_id,
            &ReplAckFrame::new(self.worker_index, stored_lsn),
        )?;
        conn.next_request_id += 1;
        conn.stream
            .write_all(&ack)
            .await
            .map_err(|e| mudu_error!(ErrorCode::Network, "write repl ack error", e))?;
        Ok(())
    }

    /// Stream mode: rejects stale-epoch batches, applies the frames
    /// directly, snapshots at the threshold and returns the stored ack
    /// watermark.
    async fn apply_stream_batch(
        &mut self,
        ctl: &ReplModeCtl,
        log: &ChunkedWorkerLogBackend,
        snapshot_threshold_bytes: u64,
        batch: ReplBatchFrame,
    ) -> RS<u64> {
        let worker = self.worker;
        let worker_index = self.worker_index;
        let addr = self.addr;
        let state = self
            .direct_state
            .as_mut()
            .ok_or_else(|| mudu_error!(ErrorCode::Internal, "replication sink state missing"))?;
        // Epoch monotonicity: a zombie primary's batch is
        // rejected; a newer epoch is recorded and persisted.
        if batch.epoch() < state.known_epoch {
            return Err(mudu_error!(
                ErrorCode::InvalidState,
                format!(
                    "stale primary stream at {addr}: batch epoch {} below the known epoch {}",
                    batch.epoch(),
                    state.known_epoch
                )
            ));
        }
        note_stream_epoch(worker, ctl, state, batch.epoch()).await?;
        apply_direct_frames(worker, log, batch.into_frames(), state).await?;
        // The ack watermark is the highest durably stored
        // frame LSN (the resume point minus one); an empty
        // stream never acks below what it holds.
        let stored = state.resume.saturating_sub(1);
        // Snapshot at this serial point once the applied
        // stream crossed the threshold.
        if stored >= *self.last_snapshot_lsn + snapshot_threshold_bytes {
            match crate::snapshot::take_worker_snapshot(
                worker,
                worker_index,
                stored,
                crate::seg_hash::ZERO_SEG_HASH,
                0,
                0,
            )
            .await
            {
                Ok(_) => *self.last_snapshot_lsn = stored,
                Err(err) => {
                    warn!(
                        worker_index,
                        "stream replica snapshot failed (will retry): {err}"
                    );
                }
            }
        }
        Ok(stored)
    }
}

/// Records a higher stream epoch seen from the primary: tracks it in the
/// resume state and the shared control, and persists it (epoch changes are
/// rare, so the fsync per change is cheap).
async fn note_stream_epoch(
    worker: &WorkerRuntime,
    ctl: &ReplModeCtl,
    state: &mut DirectResumeState,
    epoch: u64,
) -> RS<()> {
    if epoch <= state.known_epoch {
        return Ok(());
    }
    state.known_epoch = epoch;
    ctl.note_epoch(epoch);
    crate::stream_admin::store_stream_epoch(worker.storage().relation_path(), epoch)?;
    Ok(())
}

/// Applies one batch of streamed frames directly (replication mode):
/// append to the local WAL, flush, replay into storage, advance the resume
/// point. Frames below the resume point are overlap candidates after a
/// follow target change: byte-compared (CRC64) against the local WAL and
/// the first mismatch truncates the divergent tail before the new
/// primary's content replaces it.
async fn apply_direct_frames(
    worker: &WorkerRuntime,
    log: &ChunkedWorkerLogBackend,
    frames: Vec<Vec<u8>>,
    state: &mut DirectResumeState,
) -> RS<()> {
    // Build the local CRC map of the probe window once per subscription.
    if let Some(probe_from) = state.probe_from {
        state.probe_from = None;
        state.local_crcs = scan_local_crcs(log, probe_from, state.resume).await?;
    }
    let mut new_frames = Vec::new();
    for frame in frames {
        let lsn = staged_frame_lsn(&frame)?.as_u64();
        if lsn < state.resume {
            if let Some(local_crc) = state.local_crcs.get(&lsn).copied() {
                state.local_crcs.remove(&lsn);
                if local_crc == crc64(&frame) {
                    continue;
                }
                warn!(
                    lsn,
                    "stream divergence: local frame conflicts with the new primary, truncating the local tail"
                );
                log.truncate_from_lsn(LSN::new(lsn)).await?;
                state.resume = lsn;
                state.local_crcs.clear();
                state.pending_frames.clear();
                state.pending_start_lsn = None;
            } else {
                continue;
            }
        }
        if lsn != state.resume {
            return Err(mudu_error!(
                ErrorCode::InvalidState,
                format!(
                    "replication stream gap: stored up to lsn {} but the primary streamed lsn {}",
                    state.resume.saturating_sub(1),
                    lsn
                )
            ));
        }
        new_frames.push(frame);
        state.resume = lsn.saturating_add(1);
    }
    if new_frames.is_empty() {
        return Ok(());
    }
    // Ordering: append to the local WAL first, then flush, then replay —
    // the ack the caller sends afterwards implies durable storage.
    log.append_frames_async(new_frames.clone()).await?;
    log.flush_async().await?;
    let batches = decode_xl_batches_with_pending(
        &new_frames,
        &mut state.pending_frames,
        &mut state.pending_start_lsn,
    )?;
    for batch in batches {
        worker.replay_log_batch(batch).await?;
    }
    Ok(())
}

/// Maps `lsn -> CRC64` of the local WAL frames in `[lo, hi)` (the overlap
/// probe window after a follow target change).
async fn scan_local_crcs(
    log: &ChunkedWorkerLogBackend,
    lo: u64,
    hi: u64,
) -> RS<std::collections::BTreeMap<u64, u64>> {
    let mut out = std::collections::BTreeMap::new();
    if lo >= hi {
        return Ok(out);
    }
    let fs = log.fs();
    for path in log.chunk_paths_sorted().await? {
        let bytes = fs.read_all(&path).await?;
        if bytes.is_empty() {
            continue;
        }
        let prefix = ::wal::worker_log::scan_valid_frame_prefix(&bytes);
        for frame in &prefix.frames {
            let lsn = staged_frame_lsn(frame)?.as_u64();
            if lsn >= hi {
                return Ok(out);
            }
            if lsn >= lo {
                out.insert(lsn, crc64(frame));
            }
        }
    }
    Ok(out)
}

/// The sink's staging progress within one stream: the next LSN it expects
/// and the highest LSN it has staged durably (the ack watermark).
struct StagingTrack {
    next_expected: u64,
    staged_hi: Option<u64>,
}

/// Stages one batch of leader frames. Frames below the next expected LSN
/// are byte-compared (divergence heal): identical frames are skipped, and
/// the first mismatch truncates the speculative staged tail from that LSN
/// before the leader's content is staged in its place.
async fn stage_frames(
    staging: &Arc<StagingStore>,
    frames: Vec<Vec<u8>>,
    staged: &mut StagingTrack,
) -> RS<()> {
    for frame in frames {
        let lsn = staged_frame_lsn(&frame)?.as_u64();
        if lsn < staged.next_expected {
            let staged_crc = staging.frame_crc(lsn)?;
            if staged_crc == Some(crc64(&frame)) {
                continue;
            }
            warn!(
                lsn,
                "replication divergence: staged frame conflicts with the leader, truncating staged tail"
            );
            staging.truncate_from(lsn).await?;
            staged.next_expected = lsn;
            staged.staged_hi = (lsn > 0).then_some(lsn - 1);
        }
        if lsn != staged.next_expected {
            return Err(mudu_error!(
                ErrorCode::InvalidState,
                format!(
                    "replication stream gap: staged up to lsn {} but the leader streamed lsn {}",
                    staged.next_expected.saturating_sub(1),
                    lsn
                )
            ));
        }
        staging.append_frames(&[frame]).await?;
        staged.staged_hi = Some(lsn);
        staged.next_expected = lsn.saturating_add(1);
    }
    Ok(())
}

/// Maps an error frame from the leader into a local error, appending the
/// leader hint (when present) so the log line shows where to redirect.
fn repl_remote_error(error: &ErrorResponse) -> MuduError {
    let ec = ErrorCode::from_u32(error.code()).unwrap_or(ErrorCode::Internal);
    let message = match error.leader_hint() {
        Some(hint) => format!(
            "{} (leader hint {}:{})",
            error.message(),
            hint.host(),
            hint.port()
        ),
        None => error.message().to_string(),
    };
    mudu_error!(
        ec,
        format!("replication leader returned error: {}", message)
    )
}
