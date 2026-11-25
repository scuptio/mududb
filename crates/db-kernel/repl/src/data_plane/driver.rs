//! The per-worker stream commit driver on the data-group leader (G-03).
//!
//! This keeps the storage-facing `CommitSequencer` contract exactly: staged
//! prepared commits are cut into segments by the same group-commit
//! watermarks, but a segment is committed as one record on the worker's data
//! stream (`GroupCommit` progress reference in the shared Raft log, payload
//! frames through the log stream) instead of a `CommitVec` entry.
//!
//! One driver per worker guarantees lsn order == stream order == apply
//! order. On leadership loss mid-flight, staged commits fail with a
//! retryable not-leader error; their frames stay uncommitted everywhere and
//! are truncated by the convergence flow.

use crate::data_plane::codec::DataSegment;
use crate::data_plane::DataPlaneService;
use crate::wal_cursor::LogFrameCursor;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use std::sync::Arc;
use storage::prepared_commit::PreparedWorkerCommit;
use wal::lsn::LSN;
use wal::worker_log::{ChunkedWorkerLogBackend, WorkerLogBatching};
use xcontract::worker_storage::WorkerStorage;

use mudu_sys::sync::async_::stop_flag::StopRx;
use mudu_sys::tokio;
use mudu_sys::tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use mudu_sys::tokio::sync::oneshot;

/// One staged commit handed from the commit path to the driver.
struct StagedCommit {
    prepared: PreparedWorkerCommit,
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
pub struct StreamCommitDriverRef {
    tx: UnboundedSender<SeqMsg>,
}

impl storage::prepared_commit::CommitSequencer for StreamCommitDriverRef {
    fn stage(
        &self,
        prepared: PreparedWorkerCommit,
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
            .map_err(|_| mudu_error!(ErrorCode::ChannelClosed, "commit driver is not running"))?;
        Ok(wait)
    }
}

/// Spawns the worker's stream commit driver; the returned handle is handed
/// to the contract's commit path. The async driver is
/// [`run_stream_commit_driver`].
pub struct StreamCommitDriver {
    rx: UnboundedReceiver<SeqMsg>,
    worker_index: u32,
    log: ChunkedWorkerLogBackend,
    storage: Arc<WorkerStorage>,
    service: Arc<DataPlaneService>,
    batching: WorkerLogBatching,
    /// Chain tail committed by this driver (the WAL resume point after the
    /// last committed segment).
    local_tail: Option<u64>,
    cursor: LogFrameCursor,
}

/// Builds the driver handle + driver state for one worker.
pub fn new_stream_commit_driver(
    worker_index: u32,
    log: ChunkedWorkerLogBackend,
    storage: Arc<WorkerStorage>,
    service: Arc<DataPlaneService>,
) -> RS<(StreamCommitDriverRef, StreamCommitDriver)> {
    let (tx, rx) = unbounded_channel();
    let batching = log.layout()?.batching();
    Ok((
        StreamCommitDriverRef { tx },
        StreamCommitDriver {
            rx,
            worker_index,
            cursor: LogFrameCursor::new(log.clone()),
            log,
            storage,
            service,
            batching,
            local_tail: None,
        },
    ))
}

/// Runs the stream commit driver until `stop_rx` fires; staged commits that
/// are still pending then fail with a retryable not-leader error.
pub async fn run_stream_commit_driver(
    mut driver: StreamCommitDriver,
    mut stop_rx: StopRx,
) -> RS<()> {
    let result = driver.run_loop(&mut stop_rx).await;
    // Fail everything still staged so committers never park on shutdown.
    let pending = driver.take_staged();
    fail_staged(
        pending,
        mudu_error!(
            ErrorCode::InvalidState,
            "commit driver stopped before the segment committed"
        ),
    );
    result
}

impl StreamCommitDriver {
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
                tracing::warn!(
                    worker_index = self.worker_index,
                    "stream commit segment failed: {}",
                    err
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
        let Some(last) = staged.last() else {
            return Ok(());
        };
        let seg_hi = last.last_lsn.as_u64();
        // Make the segment durable locally (the inline write+fsync drains
        // the queue, covering every enqueued frame <= seg_hi).
        if let Err(err) = self.log.drive_group_commit_flush().await {
            self.service.request_convergence();
            fail_staged(staged, err);
            return Ok(());
        }
        // The resume point: this driver's own last committed tail, else the
        // data machine's applied WAL point (advanced by convergence).
        let cell = self.service.wal_resume(self.worker_index)?;
        let prev_lsn = match self.local_tail {
            Some(tail) if tail >= cell => tail,
            _ => {
                self.local_tail = None;
                self.cursor = LogFrameCursor::new(self.log.clone());
                cell
            }
        };
        if seg_hi < prev_lsn {
            self.service.request_convergence();
            fail_staged(
                staged,
                mudu_error!(
                    ErrorCode::InvalidState,
                    format!(
                        "commit segment end lsn {seg_hi} is below the committed resume point {prev_lsn}"
                    )
                ),
            );
            return Ok(());
        }
        // Pin the worker WAL read floor across the frame read so the prefix GC
        // cannot reclaim the frames this segment is about to replicate.
        self.service.pin_wal_read(self.worker_index, prev_lsn);
        let frames = match self.read_segment_frames(prev_lsn, seg_hi).await {
            Ok(frames) => {
                self.service.unpin_wal_read(self.worker_index);
                frames
            }
            Err(err) => {
                self.service.unpin_wal_read(self.worker_index);
                self.service.request_convergence();
                fail_staged(staged, err);
                return Ok(());
            }
        };
        let segment = DataSegment { version: 1, frames };
        // The group commit can wait on the quorum gate for as long as a
        // follower lags; the driver's own stop must still cut it short so a
        // staged committer never parks past shutdown.
        let appended = tokio::select! {
            result = self.service.append_data_segment(self.worker_index, prev_lsn, segment) => {
                result
            }
            _ = stop_rx.changed() => {
                Err(mudu_error!(
                    ErrorCode::InvalidState,
                    "commit driver stopped while the segment awaited the quorum gate"
                ))
            }
        };
        match appended {
            Ok(_receipt) => {
                self.local_tail = Some(seg_hi.saturating_add(1));
                if self.apply_staged(staged).await {
                    if let Err(error) = self
                        .service
                        .snapshot_after_local_apply(self.worker_index)
                        .await
                    {
                        tracing::warn!("post-apply snapshot deferred: {error}");
                    }
                }
            }
            Err(err) => {
                if !stop_rx.is_stopped() {
                    self.service.request_convergence();
                }
                fail_staged(staged, err);
            }
        }
        Ok(())
    }

    /// Reads the segment's frames `[prev_lsn, seg_hi]` (inclusive resume
    /// range) from the local WAL.
    async fn read_segment_frames(&mut self, prev_lsn: u64, seg_hi: u64) -> RS<Vec<Vec<u8>>> {
        let mut frames = Vec::new();
        loop {
            let frame = self.cursor.next_frame().await?;
            let Some(frame) = frame else {
                crate::note_gc_race_detected();
                return Err(mudu_error!(
                    ErrorCode::InvalidState,
                    format!("worker log ended before segment lsn {seg_hi} (resume at {prev_lsn})")
                ));
            };
            let lsn = wal::log_frame::frame_lsn(&frame)?.as_u64();
            if lsn < prev_lsn {
                continue;
            }
            if lsn > seg_hi {
                return Err(mudu_error!(
                    ErrorCode::InvalidState,
                    format!("worker log frame lsn {lsn} skipped past segment end {seg_hi}")
                ));
            }
            let done = lsn == seg_hi;
            frames.push(frame);
            if done {
                return Ok(frames);
            }
        }
    }

    /// Applies the staged commits in order and completes each waiter.
    async fn apply_staged(&mut self, staged: Vec<StagedCommit>) -> bool {
        let mut failure: Option<mudu::error::MuduError> = None;
        for commit in staged {
            let result = match (&failure, commit.last_lsn.as_u64().checked_add(1)) {
                (_, None) => Err(mudu_error!(ErrorCode::InvalidState, "WAL LSN exhausted")),
                (Some(err), _) => Err(mudu_error!(
                    err.ec(),
                    format!("skipped after earlier apply failure: {}", err.message())
                )),
                (None, Some(resume)) => {
                    self.storage
                        .apply_prepared_commit_at_async(commit.prepared, resume)
                        .await
                }
            };
            if let Err(err) = &result {
                tracing::warn!(
                    worker_index = self.worker_index,
                    "stream commit apply failed: {}",
                    err
                );
                failure = Some(mudu_error!(err.ec(), err.message()));
            }
            let _ = commit.done.send(result);
        }
        failure.is_none()
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
