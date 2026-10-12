//! Per-worker commit sequencer on the raft leader.
//!
//! The leader's commit path stages prepared transactions here instead of
//! applying them inline. The sequencer cuts the staged stream into segments
//! aligned with the group-commit watermarks (`trigger_frames` /
//! `trigger_bytes` / `max_wait` of the worker log's batching config), and
//! per segment:
//!
//! 1. drives the group-commit flush so the segment is durable locally,
//! 2. waits until the replication ack watermarks show a MAJORITY of voters
//!    with `stored_lsn >= segment_hi` (design invariant I1′: a committed
//!    hash implies the data is on a majority),
//! 3. hashes the segment's WAL bytes and proposes a `CommitVec` carrying the
//!    full per-worker committed vector,
//! 4. waits for the proposal to commit, applies each prepared batch IN
//!    ORDER and completes each staged commit, which then proceeds to
//!    drive-flush / wait-durable / ack on the caller exactly as the
//!    single-node path does.
//!
//! One sequencer per worker guarantees lsn order == raft order == apply
//! order. On leadership loss mid-flight, staged commits fail with a
//! retryable not-leader error; their frames stay uncommitted everywhere and
//! are truncated by the healing flows.

use crate::raft_service::RaftShare;
use crate::seg_hash::{seg_hash_chain, SegHash};
use crate::wal_cursor::LogFrameCursor;
use crate::{ack_watermarks, ReplWorkerState};
use ::catalog::contract::message_bus_api::ServerInstanceId;
use ::wal::lsn::LSN;
use ::wal::worker_log::{ChunkedWorkerLogBackend, WorkerLogBatching};
use ::xcontract::worker_storage::WorkerStorage;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_raft::core::role::RaftRole;
use mudu_raft::{RaftHandle, ReplEntry};
use mudu_sys::sync::async_::stop_flag::StopRx;
use mudu_sys::tokio;
use mudu_sys::tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use mudu_sys::tokio::sync::oneshot;
use std::sync::Arc;
use std::time::Duration;
use tracing::{trace, warn};

/// Poll interval while waiting for majority ack coverage of a segment.
const MAJORITY_WAIT_POLL: Duration = Duration::from_millis(2);

/// One staged commit handed from the commit path to the sequencer.
struct StagedCommit {
    prepared: ::storage::prepared_commit::PreparedWorkerCommit,
    /// Number of WAL frames the commit enqueued (batching watermark).
    frame_count: usize,
    /// Total byte length of the enqueued frames (batching watermark).
    byte_len: usize,
    last_lsn: LSN,
    done: oneshot::Sender<RS<()>>,
}

enum SeqMsg {
    Stage(StagedCommit),
}

/// Cheap cloneable handle the contract's commit path stages commits through.
#[derive(Clone)]
pub struct CommitSequencerRef {
    tx: UnboundedSender<SeqMsg>,
}

impl CommitSequencerRef {
    /// Stages a prepared commit (already enqueued into the group-commit
    /// queue with `last_lsn` allocated) and returns a receiver that
    /// completes once the commit is raft-committed and applied.
    pub(crate) fn stage(
        &self,
        prepared: ::storage::prepared_commit::PreparedWorkerCommit,
        frame_count: usize,
        byte_len: usize,
        last_lsn: LSN,
    ) -> RS<oneshot::Receiver<RS<()>>> {
        let (done, wait) = oneshot::channel();
        self.tx
            .send(SeqMsg::Stage(StagedCommit {
                prepared,
                frame_count,
                byte_len,
                last_lsn,
                done,
            }))
            .map_err(|_| {
                mudu_error!(ErrorCode::ChannelClosed, "commit sequencer is not running")
            })?;
        Ok(wait)
    }
}

impl ::storage::prepared_commit::CommitSequencer for CommitSequencerRef {
    fn stage(
        &self,
        prepared: ::storage::prepared_commit::PreparedWorkerCommit,
        frame_count: usize,
        byte_len: usize,
        last_lsn: LSN,
    ) -> RS<oneshot::Receiver<RS<()>>> {
        CommitSequencerRef::stage(self, prepared, frame_count, byte_len, last_lsn)
    }
}

/// Spawns the worker's commit sequencer state; the returned handle is handed
/// to the contract's commit path. The async driver is [`run_commit_sequencer`].
pub struct CommitSequencer {
    rx: UnboundedReceiver<SeqMsg>,
    server_instance_id: ServerInstanceId,
    worker_index: u32,
    log: ChunkedWorkerLogBackend,
    storage: Arc<WorkerStorage>,
    raft: Arc<RaftHandle>,
    share: Arc<RaftShare>,
    state: Arc<ReplWorkerState>,
    batching: WorkerLogBatching,
    /// Chain tail proposed by this sequencer (`None` before the first
    /// segment: the chain then resumes from the applier's cell, which the
    /// leader-elect heal has advanced to the healed vector).
    local_tail: Option<(u64, SegHash)>,
    cursor: LogFrameCursor,
}

/// Builds the sequencer handle + driver state for one worker.
pub fn new_commit_sequencer(
    server_instance_id: ServerInstanceId,
    worker_index: u32,
    log: ChunkedWorkerLogBackend,
    storage: Arc<WorkerStorage>,
    raft: Arc<RaftHandle>,
    share: Arc<RaftShare>,
    state: Arc<ReplWorkerState>,
) -> RS<(CommitSequencerRef, CommitSequencer)> {
    let (tx, rx) = unbounded_channel();
    let batching = log.layout()?.batching();
    Ok((
        CommitSequencerRef { tx },
        CommitSequencer {
            rx,
            server_instance_id,
            worker_index,
            cursor: LogFrameCursor::new(log.clone()),
            log,
            storage,
            raft,
            share,
            state,
            batching,
            local_tail: None,
        },
    ))
}

/// Runs the commit sequencer until `stop_rx` fires; staged commits that are
/// still pending then fail with a retryable not-leader error.
pub async fn run_commit_sequencer(mut sequencer: CommitSequencer, mut stop_rx: StopRx) -> RS<()> {
    let result = sequencer.run_loop(&mut stop_rx).await;
    // Fail everything still staged so committers never park on shutdown.
    let pending = sequencer.take_staged();
    fail_staged(
        pending,
        mudu_error!(
            ErrorCode::InvalidState,
            "commit sequencer stopped before the segment committed"
        ),
    );
    result
}

impl CommitSequencer {
    async fn run_loop(&mut self, stop_rx: &mut StopRx) -> RS<()> {
        loop {
            if stop_rx.is_stopped() {
                return Ok(());
            }
            let first = tokio::select! {
                msg = self.rx.recv() => {
                    match msg {
                        Some(SeqMsg::Stage(commit)) => commit,
                        None => return Ok(()),
                    }
                }
                _ = stop_rx.changed() => {
                    return Ok(());
                }
            };
            let mut staged = vec![first];
            self.accumulate_segment(&mut staged, stop_rx).await;
            if staged.is_empty() {
                continue;
            }
            if let Err(err) = self.finalize_segment(staged, stop_rx).await {
                warn!(
                    worker_index = self.worker_index,
                    "commit sequencer segment failed: {}", err
                );
            }
        }
    }

    /// Drains the inbox into the current segment until a watermark
    /// (`trigger_frames` / `trigger_bytes`) is hit or the batching window
    /// (`max_wait`) expires.
    async fn accumulate_segment(&mut self, staged: &mut Vec<StagedCommit>, stop_rx: &mut StopRx) {
        let started = mudu_sys::time::instant_now();
        loop {
            let (frames, bytes) = staged_stats(staged);
            if frames >= self.batching.trigger_frames || bytes >= self.batching.trigger_bytes {
                return;
            }
            let elapsed = started.elapsed();
            if elapsed >= self.batching.max_wait {
                return;
            }
            let remaining = self.batching.max_wait - elapsed;
            tokio::select! {
                msg = self.rx.recv() => {
                    match msg {
                        Some(SeqMsg::Stage(commit)) => staged.push(commit),
                        None => return,
                    }
                }
                _ = mudu_sys::task::async_::sleep(remaining) => {
                    return;
                }
                _ = stop_rx.changed() => {
                    return;
                }
            }
        }
    }

    async fn finalize_segment(
        &mut self,
        staged: Vec<StagedCommit>,
        stop_rx: &mut StopRx,
    ) -> RS<()> {
        let seg_hi = staged
            .last()
            .map(|commit| commit.last_lsn.as_u64())
            .ok_or_else(|| mudu_error!(ErrorCode::Internal, "empty commit segment"))?;
        // Make the segment durable locally (the inline write+fsync drains
        // the queue, covering every enqueued frame <= seg_hi).
        if let Err(err) = self.log.drive_group_commit_flush().await {
            fail_staged(staged, err);
            return Ok(());
        }
        trace!(
            worker_index = self.worker_index,
            seg_hi,
            "commit segment flushed, awaiting majority"
        );
        if let Err(err) = self.wait_majority_covering(seg_hi, stop_rx).await {
            fail_staged(staged, err);
            return Ok(());
        }
        trace!(
            worker_index = self.worker_index,
            seg_hi,
            "commit segment majority covered"
        );
        // Chain prev: normally this sequencer's own last committed tail
        // (the applier lags its cell slightly behind it); when the cell
        // moved PAST the local tail, the leader-elect heal advanced the
        // chain (possibly with a WAL truncation), so resume from the cell
        // and rebuild the WAL cursor.
        let cell = self.state.chain()?;
        if seg_hi < cell.0 {
            // LSN collision: the segment would overwrite committed chain
            // content. Fail loudly instead of hanging the cluster on a
            // poisoned hash chain; the next heal re-anchors the sequencer.
            self.local_tail = None;
            fail_staged(
                staged,
                mudu_error!(
                    ErrorCode::InvalidState,
                    format!(
                        "commit segment end lsn {} is below the committed resume point {} (chain collision)",
                        seg_hi, cell.0
                    )
                ),
            );
            return Ok(());
        }
        let (prev_lsn, prev_hash) = self.chain_prev(cell);
        let seg_frames = match self.read_segment_frames(prev_lsn, seg_hi).await {
            Ok(frames) => frames,
            Err(err) => {
                // The chain and the local WAL disagree; staged commits are
                // retryable elsewhere after the next heal.
                self.local_tail = None;
                fail_staged(staged, err);
                return Ok(());
            }
        };
        let seg_hash = seg_hash_chain(&prev_hash, &seg_frames);
        let waiter = match self.propose_commit_vec(seg_hi, seg_hash) {
            Ok(waiter) => waiter,
            Err(err) => {
                fail_staged(staged, err);
                return Ok(());
            }
        };
        match waiter.recv() {
            Ok((_index, _term)) => {
                trace!(
                    worker_index = self.worker_index,
                    seg_hi,
                    "commit segment raft-committed"
                );
                // The chain cell is a resume point: the next LSN after this
                // segment's highest frame.
                self.local_tail = Some((seg_hi.saturating_add(1), seg_hash));
                self.apply_staged(staged).await;
            }
            Err(err) => {
                fail_staged(staged, err.into_mudu_error());
            }
        }
        Ok(())
    }

    /// Resolves the chain prev point for the segment to propose: this
    /// sequencer's own last committed tail while it still covers the chain
    /// cell, else the cell itself (with a rebuilt WAL cursor).
    fn chain_prev(&mut self, cell: (u64, SegHash)) -> (u64, SegHash) {
        match self.local_tail {
            Some(tail) if tail.0 >= cell.0 => tail,
            _ => {
                self.local_tail = None;
                self.cursor = LogFrameCursor::new(self.log.clone());
                (cell.0, cell.1)
            }
        }
    }

    /// Proposes the CommitVec carrying this worker's new committed point
    /// alongside every other worker's latest known point.
    fn propose_commit_vec(
        &self,
        seg_hi: u64,
        seg_hash: SegHash,
    ) -> RS<mudu_raft::RaftProposeWaiter> {
        let term = self.share.term();
        let mut vector = self.share.committed_vector();
        vector.insert(self.worker_index, (seg_hi, seg_hash));
        let per_worker: Vec<(u32, u64, SegHash)> = vector
            .into_iter()
            .map(|(worker, (lsn, hash))| (worker, lsn, hash))
            .collect();
        self.raft.propose(ReplEntry::CommitVec { term, per_worker })
    }

    /// Waits until a majority of voters (this node plus followers' repl ack
    /// watermarks) stores `seg_hi`, or the sequencer is stopped.
    async fn wait_majority_covering(&self, seg_hi: u64, stop_rx: &mut StopRx) -> RS<()> {
        let quorum = self.share.quorum_size();
        loop {
            if stop_rx.is_stopped() {
                return Err(mudu_error!(
                    ErrorCode::InvalidState,
                    "commit sequencer stopped while waiting for the majority"
                ));
            }
            if self.share.role() != RaftRole::Leader {
                return Err(not_leader_error(&self.share));
            }
            let watermarks = ack_watermarks(self.server_instance_id, self.worker_index)?;
            let voters: std::collections::HashSet<u64> =
                self.share.voter_ids().into_iter().collect();
            let covered = 1 + watermarks
                .iter()
                .filter(|(node, stored)| *stored >= seg_hi && voters.contains(node))
                .count();
            trace!(
                worker_index = self.worker_index,
                seg_hi,
                covered,
                quorum,
                ?watermarks,
                "commit majority wait"
            );
            if covered >= quorum {
                return Ok(());
            }
            tokio::select! {
                _ = self.state.ack_notified() => {}
                _ = mudu_sys::task::async_::sleep(MAJORITY_WAIT_POLL) => {}
                _ = stop_rx.changed() => {}
            }
        }
    }

    /// Reads the segment's frames `[prev_lsn, seg_hi]` (inclusive resume
    /// range) from the local WAL — the hash input, byte-identical to what
    /// followers stage.
    async fn read_segment_frames(&mut self, prev_lsn: u64, seg_hi: u64) -> RS<Vec<Vec<u8>>> {
        let mut frames = Vec::new();
        loop {
            let frame = self.cursor.next_frame().await?;
            let Some(frame) = frame else {
                return Err(mudu_error!(
                    ErrorCode::InvalidState,
                    format!(
                        "worker log ended before segment lsn {} (chain resume at {})",
                        seg_hi, prev_lsn
                    )
                ));
            };
            let lsn = ::wal::log_frame::frame_lsn(&frame)?.as_u64();
            if lsn < prev_lsn {
                continue;
            }
            if lsn > seg_hi {
                return Err(mudu_error!(
                    ErrorCode::InvalidState,
                    format!(
                        "worker log frame lsn {} skipped past segment end {} (chain resume at {})",
                        lsn, seg_hi, prev_lsn
                    )
                ));
            }
            let done = lsn == seg_hi;
            frames.push(frame);
            if done {
                return Ok(frames);
            }
        }
    }

    /// Applies the staged commits in order and completes each waiter; the
    /// caller-side commit then continues to drive-flush / wait-durable.
    async fn apply_staged(&mut self, staged: Vec<StagedCommit>) {
        let mut failure: Option<mudu::error::MuduError> = None;
        for commit in staged {
            let result = match &failure {
                Some(err) => Err(mudu_error!(
                    err.ec(),
                    format!("skipped after earlier apply failure: {}", err.message())
                )),
                None => {
                    self.storage
                        .apply_prepared_commit_async(commit.prepared)
                        .await
                }
            };
            if let Err(err) = &result {
                warn!(
                    worker_index = self.worker_index,
                    "commit sequencer apply failed: {}", err
                );
                failure = Some(mudu_error!(err.ec(), err.message()));
            }
            let _ = commit.done.send(result);
        }
    }

    fn take_staged(&mut self) -> Vec<StagedCommit> {
        let mut staged = Vec::new();
        while let Ok(SeqMsg::Stage(commit)) = self.rx.try_recv() {
            staged.push(commit);
        }
        staged
    }
}

fn staged_stats(staged: &[StagedCommit]) -> (usize, usize) {
    let frames = staged.iter().map(|commit| commit.frame_count).sum();
    let bytes = staged.iter().map(|commit| commit.byte_len).sum();
    (frames, bytes)
}

fn fail_staged(staged: Vec<StagedCommit>, err: mudu::error::MuduError) {
    let message = err.message().to_string();
    let ec = err.ec();
    for commit in staged {
        let _ = commit.done.send(Err(mudu_error!(ec, message.clone())));
    }
}

fn not_leader_error(share: &RaftShare) -> mudu::error::MuduError {
    match share.leader_id() {
        Some(leader) => mudu_error!(
            ErrorCode::InvalidState,
            format!("not the raft leader (leader hint: node {leader})")
        ),
        None => mudu_error!(
            ErrorCode::InvalidState,
            "not the raft leader (leader unknown)"
        ),
    }
}
