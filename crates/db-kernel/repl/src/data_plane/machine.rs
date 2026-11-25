//! The business state machines of a data group's streams: the per-worker
//! data stream (promote committed segments into the worker WAL and replay
//! them into storage) and the `tx_decision` stream (the cross-shard
//! transaction decision store of the coordinator's data group).

use super::codec::DataSegment;
use crate::log_stream::{FrontierStore, StreamStateMachine, StreamStruct};
use async_trait::async_trait;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_raft::stream::StreamProgress;
use mudu_raft::{RaftHandle, RaftRole};
use mudu_sys::sync::SMutex;
use std::sync::Arc;
use tracing::warn;
use wal::log_frame::frame_lsn;
use wal::lsn::LSN;
use wal::worker_log::{ChunkedWorkerLogBackend, WorkerLogBackend};
use wal::xl_batch::decode_xl_batches_with_pending;
use worker::worker::WorkerRuntime;

/// The stream id of a data group's `tx_decision` stream. Sits after the
/// control-plane stream ids (meta = 1, epoch = 2) so the control and data
/// streams can share one replication group without colliding (S-02).
pub const TX_DECISION_STREAM: mudu_raft::stream::StreamId = mudu_raft::stream::StreamId(3);

/// The durable business checkpoint of a worker's data stream: the committed
/// WAL resume point (the next LSN to commit), stored with the applied
/// stream frontier.
#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
pub struct DataCheckpoint {
    /// The committed WAL resume point after the applied record.
    pub wal_resume: u64,
}

/// Promotes committed data segments into the worker's WAL and replays them
/// into storage. On the leader the segment's frames are already in the local
/// WAL (the commit path wrote them), so the promote is a pure chain advance
/// with no replay (the leader applied the prepared write sets at commit
/// time); on a follower the frames are appended and replayed.
pub struct DataStreamMachine {
    worker: WorkerRuntime,
    worker_index: u32,
    log: ChunkedWorkerLogBackend,
    raft: Arc<RaftHandle>,
    frontier: FrontierStore,
    /// The stream digest and its WAL position are published/read together.
    applied: SMutex<AppliedDataCut>,
    /// XL reassembly buffer carried across records.
    pending_decode: SMutex<(Vec<Vec<u8>>, Option<LSN>)>,
    /// Committed WAL bytes beyond the last snapshot that trigger the next.
    snapshot_threshold_bytes: u64,
    /// The committed LSN of this worker's latest snapshot.
    last_snapshot_lsn: SMutex<Option<u64>>,
}

struct AppliedDataCut {
    progress: Option<StreamProgress>,
    wal_resume: u64,
}

impl DataStreamMachine {
    /// Opens the machine: recovers the applied frontier and the committed
    /// WAL resume point, truncating any former-leader speculative WAL tail.
    pub async fn open(
        worker: WorkerRuntime,
        raft: Arc<RaftHandle>,
        apply_dir: &std::path::Path,
        file_name: &str,
        stream_id: mudu_raft::stream::StreamId,
        snapshot_threshold_bytes: u64,
    ) -> RS<Self> {
        let worker_index = u32::try_from(worker.worker_index()).map_err(|_| {
            mudu_error!(
                ErrorCode::InvalidState,
                "worker index overflows the replication protocol"
            )
        })?;
        let log = worker.worker_log()?.ok_or_else(|| {
            mudu_error!(ErrorCode::InvalidState, "data machine: worker has no log")
        })?;
        let (frontier, recovered) = FrontierStore::open(apply_dir, file_name, stream_id, 1).await?;
        let wal_resume = match &recovered {
            Some((_, business)) => {
                let checkpoint: DataCheckpoint = crate::log_stream::frontier::decode(business)?;
                checkpoint.wal_resume
            }
            None => {
                crate::scan_worker_log_tail(
                    &log,
                    crate::snapshot::recovery_base_resume(&worker)?.unwrap_or(0),
                )
                .await?
                .0
            }
        };
        if log.next_lsn().as_u64() > wal_resume {
            log.truncate_from_lsn(LSN::new(wal_resume)).await?;
        }
        let last_snapshot_lsn =
            crate::snapshot::load_latest_manifest(worker.storage().relation_path(), worker_index)?
                .map(|manifest| manifest.committed_lsn);
        Ok(Self {
            worker,
            worker_index,
            log,
            raft,
            frontier,
            applied: SMutex::new(AppliedDataCut {
                progress: recovered.map(|(progress, _)| progress),
                wal_resume,
            }),
            pending_decode: SMutex::new((Vec::new(), None)),
            snapshot_threshold_bytes,
            last_snapshot_lsn: SMutex::new(last_snapshot_lsn),
        })
    }

    /// The committed WAL resume point (observability).
    pub fn wal_resume(&self) -> RS<u64> {
        Ok(self.applied.lock()?.wal_resume)
    }

    /// Promotes one committed segment: verifies WAL continuity, appends the
    /// frames not already in the local WAL, flushes them durable and replays
    /// their XL batches into storage in order, then persists the applied
    /// frontier with the new WAL resume point.
    async fn promote(&self, record: StreamStruct) -> RS<()> {
        let segment = DataSegment::decode(&record.payload)?;
        if segment.version != 1 {
            return Err(mudu_error!(
                ErrorCode::InvalidState,
                "unknown data segment version"
            ));
        }
        let (_lo, _hi) = segment.lsn_range()?;
        let mut resume = self.wal_resume()?;
        // A former leader's flushed-but-uncommitted tail must be cut before
        // the WAL extends past the committed point — but only a non-leader
        // may cut (a live leader's allocator legitimately runs ahead).
        let mut wal_hi = self.log.next_lsn().as_u64();
        if wal_hi > resume && self.raft.status().role != RaftRole::Leader {
            self.log.truncate_from_lsn(LSN::new(resume)).await?;
            wal_hi = self.log.next_lsn().as_u64();
        }
        let mut new_frames = Vec::new();
        for frame in segment.frames {
            let lsn = frame_lsn(&frame)?.as_u64();
            if lsn < resume {
                // Already covered by the committed chain; verified by the
                // stream's digest chain.
                continue;
            }
            if lsn != resume {
                return Err(mudu_error!(
                    ErrorCode::InvalidState,
                    format!("data segment breaks WAL continuity: expected {resume}, got {lsn}")
                ));
            }
            // Append and replay only frames not already in the local WAL:
            // on the leader the commit path already wrote them (and applied
            // the prepared write sets), so the promote is a pure chain
            // advance there; on a follower the frames are new.
            if lsn >= wal_hi {
                new_frames.push(frame);
            }
            resume = lsn.saturating_add(1);
        }
        if !new_frames.is_empty() {
            self.log.append_frames_async(new_frames.clone()).await?;
            self.log.flush_async().await?;
            let (batches, complete) = {
                let mut pending = self
                    .pending_decode
                    .lock()
                    .map_err(|_| mudu_error!(ErrorCode::Internal, "data decode lock poisoned"))?;
                let (pending_frames, pending_start) = &mut *pending;
                let batches =
                    decode_xl_batches_with_pending(&new_frames, pending_frames, pending_start)?;
                (batches, pending_frames.is_empty())
            };
            let count = batches.len();
            for (index, batch) in batches.into_iter().enumerate() {
                if complete && index + 1 == count {
                    self.worker.replay_log_batch_at(batch, resume).await?;
                } else {
                    self.worker.replay_log_batch(batch).await?;
                }
            }
        }
        let checkpoint = DataCheckpoint { wal_resume: resume };
        self.frontier
            .store(
                &record.progress,
                &crate::log_stream::frontier::encode(&checkpoint)?,
            )
            .await?;
        *self.applied.lock()? = AppliedDataCut {
            progress: Some(record.progress.clone()),
            wal_resume: resume,
        };
        self.maybe_snapshot(resume, record.progress.segment_digest)
            .await;
        Ok(())
    }

    pub(super) async fn snapshot_after_local_apply(&self) -> RS<()> {
        let cut = {
            let applied = self.applied.lock()?;
            applied
                .progress
                .as_ref()
                .map(|prefix| (applied.wal_resume, prefix.segment_digest))
        };
        if let Some((resume, digest)) = cut {
            self.maybe_snapshot(resume, digest).await;
        }
        Ok(())
    }

    /// Takes a worker snapshot once the committed point moved at least
    /// `snapshot_threshold_bytes` past the last one. Snapshot failures only
    /// log: promotion must not stall on a GC-side concern. The raft install
    /// frontier is intentionally not advanced here (B-06 pairs the registry
    /// checkpoint with the business snapshot first).
    async fn maybe_snapshot(&self, resume: u64, seg_hash: [u8; 32]) {
        let committed_lsn = resume.saturating_sub(1);
        let should = match self.last_snapshot_lsn.lock() {
            Ok(last) => match *last {
                Some(last) => {
                    committed_lsn > last && committed_lsn - last >= self.snapshot_threshold_bytes
                }
                None => resume > 0 && committed_lsn >= self.snapshot_threshold_bytes,
            },
            Err(_) => false,
        };
        if !should {
            return;
        }
        let status = self.raft.status();
        let result = crate::snapshot::take_worker_snapshot(
            &self.worker,
            self.worker_index,
            committed_lsn,
            seg_hash,
            status.term,
            status.commit_index,
        )
        .await;
        match result {
            Ok(_) => {
                if let Ok(mut last) = self.last_snapshot_lsn.lock() {
                    *last = Some(committed_lsn);
                }
            }
            Err(err) => {
                warn!(
                    committed_lsn,
                    "worker snapshot failed (will retry at the next threshold): {err}"
                );
            }
        }
    }
}

/// Reads the persisted committed WAL resume point of a worker's data stream
/// without opening the machine (startup truncation before the group starts).
pub async fn read_wal_resume_point(frontier_path: &std::path::Path) -> RS<Option<u64>> {
    let Some(business) = crate::log_stream::frontier::read_frontier_business(frontier_path).await?
    else {
        return Ok(None);
    };
    let checkpoint: DataCheckpoint = crate::log_stream::frontier::decode(&business)?;
    Ok(Some(checkpoint.wal_resume))
}

#[async_trait]
impl StreamStateMachine for DataStreamMachine {
    async fn applied_frontier(&self) -> RS<Option<StreamProgress>> {
        Ok(self
            .applied
            .lock()
            .map_err(|_| mudu_error!(ErrorCode::Internal, "data machine lock poisoned"))?
            .progress
            .clone())
    }

    async fn apply_once(&self, _request_id: u128, record: StreamStruct) -> RS<()> {
        if let Err(error) = self.promote(record).await {
            warn!("data segment apply failed: {}", error);
            return Err(mudu_error!(
                ErrorCode::InvalidState,
                "data stream apply failed"
            ));
        }
        Ok(())
    }
}
