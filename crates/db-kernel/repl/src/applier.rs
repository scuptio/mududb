//! Promote applier: moves raft-committed frames from staging into the
//! worker WAL and replays them into storage, per worker.
//!
//! The applier is the only writer of the worker WAL on a follower (design
//! invariant I2: apply <= committed vector point). For every committed
//! point `(lsn, seg_hash)` from a `CommitVec` it recomputes the segment hash
//! chain over the local frames, verifies it against the committed hash,
//! appends the not-yet-present frames to the worker WAL, flushes (so the
//! promoted prefix is durable), replays the decoded XL batches in order,
//! and persists the new committed point. The same flow runs on the leader,
//! where every frame is already in the local WAL (a pure chain-advance).
//!
//! A former leader's flushed-but-uncommitted tail is truncated lazily
//! before the first promote that would extend the WAL past the committed
//! point.

use crate::raft_service::RaftShare;
use crate::seg_hash::{seg_hash_chain, SegHash, ZERO_SEG_HASH};
use crate::staging::{PersistedCommittedPoint, StagingStore};
use crate::wal_cursor::LogFrameCursor;
use crate::{scan_worker_log_tail, ApplierMsg, ReplWorkerState};
use ::wal::log_frame::frame_lsn;
use ::wal::lsn::LSN;
use ::wal::worker_log::{ChunkedWorkerLogBackend, WorkerLogBackend};
use ::wal::xl_batch::decode_xl_batches_with_pending;
use ::worker::worker::WorkerRuntime;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_raft::RaftHandle;
use mudu_sys::sync::async_::stop_flag::StopRx;
use mudu_sys::tokio;
use mudu_sys::tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver};
use mudu_sys::tokio::sync::oneshot;
use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;
use tracing::{trace, warn};

/// The worker's replication index as `u32` (the protocol's worker id).
pub(crate) fn worker_index_of(worker: &WorkerRuntime) -> RS<u32> {
    u32::try_from(worker.worker_index()).map_err(|_| {
        mudu_error!(
            ErrorCode::Parse,
            format!(
                "worker index {} does not fit the replication protocol",
                worker.worker_index()
            )
        )
    })
}

/// Runs the promote applier of one worker until `stop_rx` fires.
pub async fn run_repl_applier(
    worker: WorkerRuntime,
    share: Arc<RaftShare>,
    state: Arc<ReplWorkerState>,
    staging: Arc<StagingStore>,
    raft: Arc<RaftHandle>,
    snapshot_threshold_bytes: u64,
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
    let Some(log) = worker.worker_log()? else {
        warn!(
            worker_index,
            "promote applier: worker has no worker log, not starting"
        );
        return Ok(());
    };
    let (tx, rx) = unbounded_channel();
    state.set_applier_tx(tx)?;
    let mut applier = ReplApplier::new(
        worker,
        share,
        state,
        staging,
        log,
        raft,
        snapshot_threshold_bytes,
    )
    .await?;
    trace!(
        worker_index,
        chain_lsn = applier.chain.0,
        "promote applier starting"
    );
    applier.run(rx, &mut stop_rx).await
}

/// A committed point the applier still has to cover: `(lsn, seg_hash,
/// raft_term, raft_index, respond)`.
type PendingPromote = (u64, SegHash, u64, u64, Option<oneshot::Sender<RS<()>>>);

struct ReplApplier {
    worker: WorkerRuntime,
    share: Arc<RaftShare>,
    state: Arc<ReplWorkerState>,
    staging: Arc<StagingStore>,
    log: ChunkedWorkerLogBackend,
    raft: Arc<RaftHandle>,
    /// Committed chain point (resume lsn + hash; mirrors the shared cell —
    /// this task is the single writer).
    chain: (u64, SegHash),
    /// Monotonic reader over the worker WAL (hash recomputation).
    wal_cursor: LogFrameCursor,
    /// XL reassembly buffer across promotes.
    pending_decode: Vec<Vec<u8>>,
    pending_start: Option<LSN>,
    /// Committed frames injected by the leader-elect heal, keyed by LSN.
    injected: BTreeMap<u64, Vec<u8>>,
    /// Committed targets waiting for coverage.
    pending_targets: VecDeque<PendingPromote>,
    /// The previous completed promote point; snapshotted on the NEXT
    /// completed promote (see `process_targets`).
    last_promoted: Option<(u64, SegHash, u64, u64)>,
    /// Committed LSN of the latest snapshot taken by this worker.
    last_snapshot_lsn: u64,
    /// Committed WAL bytes beyond the last snapshot that trigger the next.
    snapshot_threshold_bytes: u64,
}

enum PromoteOutcome {
    Done,
    /// Frames are not all present locally yet (stream lagging); retry on
    /// the next staging wake, or let the heal flow fetch them.
    NotCovered,
    Fatal(mudu::error::MuduError),
}

impl ReplApplier {
    async fn new(
        worker: WorkerRuntime,
        share: Arc<RaftShare>,
        state: Arc<ReplWorkerState>,
        staging: Arc<StagingStore>,
        log: ChunkedWorkerLogBackend,
        raft: Arc<RaftHandle>,
        snapshot_threshold_bytes: u64,
    ) -> RS<Self> {
        // Chain convention: the cell is the RESUME POINT — the next LSN to
        // be committed — and segments cover the inclusive range
        // `[chain_lsn, target_lsn]`. The backend allocator is the WAL
        // watermark: frames with an LSN below `next_lsn()` are present.
        let (wal_next, _, _) = scan_worker_log_tail(&log).await?;
        // Trust anchor: the persisted committed point, else the genesis at
        // the local WAL end (fresh cluster: every node bootstrapped the same
        // deterministic prefix and no CommitVec exists yet).
        let persisted = StagingStore::load_committed_point(staging.dir())?;
        let chain = match persisted {
            Some(point) => (point.lsn, point.hash),
            None => (wal_next, ZERO_SEG_HASH),
        };
        if log.next_lsn().as_u64() > chain.0 {
            // Former leader's uncommitted flushed tail: cut before any
            // promote extends the WAL with the new leader's content.
            log.truncate_from_lsn(LSN::new(chain.0)).await?;
            if log.next_lsn().as_u64() > chain.0 {
                return Err(mudu_error!(
                    ErrorCode::InvalidState,
                    format!(
                        "worker log truncation to committed resume lsn {} left next lsn {}",
                        chain.0,
                        log.next_lsn().as_u64()
                    )
                ));
            }
        }
        state.set_chain(chain.0, chain.1)?;
        let (_, pending_decode, pending_start) = scan_worker_log_tail(&log).await?;
        let last_snapshot_lsn = crate::snapshot::load_latest_manifest(
            worker.storage().relation_path(),
            worker_index_of(&worker)?,
        )?
        .map(|manifest| manifest.committed_lsn)
        .unwrap_or(0);
        Ok(Self {
            worker,
            share,
            state,
            staging,
            log: log.clone(),
            raft,
            chain,
            wal_cursor: LogFrameCursor::new(log),
            pending_decode,
            pending_start,
            injected: BTreeMap::new(),
            pending_targets: VecDeque::new(),
            last_promoted: None,
            last_snapshot_lsn,
            snapshot_threshold_bytes,
        })
    }

    async fn run(&mut self, mut rx: UnboundedReceiver<ApplierMsg>, stop_rx: &mut StopRx) -> RS<()> {
        loop {
            self.process_targets().await;
            tokio::select! {
                msg = rx.recv() => {
                    match msg {
                        Some(ApplierMsg::Promote { lsn, hash, raft_term, raft_index, respond }) => {
                            self.pending_targets.push_back((lsn, hash, raft_term, raft_index, respond));
                        }
                        Some(ApplierMsg::HealFrames { frames }) => {
                            self.inject_frames(frames)?;
                        }
                        Some(ApplierMsg::DiscardStaging { respond }) => {
                            let result = self.discard_staging().await;
                            if let Some(respond) = respond {
                                let _ = respond.send(result);
                            }
                        }
                        Some(ApplierMsg::InstallSnapshot { manifest, snapshot_dir, respond }) => {
                            let result = self.install_snapshot(manifest, &snapshot_dir).await;
                            let _ = respond.send(result);
                        }
                        None => return Ok(()),
                    }
                }
                _ = self.state.staging_notified() => {}
                _ = stop_rx.changed() => {
                    return Ok(());
                }
            }
        }
    }

    /// Processes queued committed targets in order until one cannot be
    /// covered yet or every target is promoted.
    async fn process_targets(&mut self) {
        while let Some((lsn_t, hash_t, raft_term, raft_index, respond)) =
            self.pending_targets.pop_front()
        {
            match self.try_promote(lsn_t, hash_t).await {
                PromoteOutcome::Done => {
                    // Snapshot the PREVIOUS promote point, not this one:
                    // on the leader the sequencer applies segment k before
                    // proposing segment k+1, so when CommitVec(k+1) is
                    // delivered the segment-k apply is complete; on a
                    // follower the apply is part of the promote itself.
                    // Snapshotting the current point could capture a
                    // not-yet-applied leader segment.
                    let done_point = (lsn_t, hash_t, raft_term, raft_index);
                    if let Some((p_lsn, p_hash, p_term, p_index)) =
                        self.last_promoted.replace(done_point)
                    {
                        self.maybe_snapshot(p_lsn, p_hash, p_term, p_index).await;
                    }
                    if let Some(respond) = respond {
                        let _ = respond.send(Ok(()));
                    }
                }
                PromoteOutcome::NotCovered => {
                    match respond {
                        // Heal-driven promote: report immediately so the
                        // heal flow fetches the missing frames from a voter.
                        Some(respond) => {
                            let _ = respond.send(Err(mudu_error!(
                                ErrorCode::EntityNotFound,
                                format!(
                                    "committed frames up to lsn {} are not all staged locally",
                                    lsn_t
                                )
                            )));
                        }
                        None => {
                            self.pending_targets
                                .push_front((lsn_t, hash_t, raft_term, raft_index, None));
                            return;
                        }
                    }
                }
                PromoteOutcome::Fatal(err) => {
                    warn!(chain_lsn = self.chain.0, "promote failed fatally: {}", err);
                    if let Some(respond) = respond {
                        let _ = respond.send(Err(err));
                    }
                    return;
                }
            }
        }
    }

    /// Takes a snapshot when the committed point moved at least
    /// `snapshot_threshold_bytes` past the last one, then advances the raft
    /// install frontier to the CommitVec carrying that point. Snapshot
    /// failures only log: promotion must not stall on a GC-side concern.
    async fn maybe_snapshot(
        &mut self,
        lsn_t: u64,
        hash_t: SegHash,
        raft_term: u64,
        raft_index: u64,
    ) {
        if lsn_t < self.last_snapshot_lsn + self.snapshot_threshold_bytes {
            return;
        }
        let worker_index = match worker_index_of(&self.worker) {
            Ok(index) => index,
            Err(err) => {
                warn!("snapshot skipped: {err}");
                return;
            }
        };
        let result = crate::snapshot::take_worker_snapshot(
            &self.worker,
            worker_index,
            lsn_t,
            hash_t,
            raft_term,
            raft_index,
        )
        .await;
        match result {
            Ok(manifest) => {
                self.last_snapshot_lsn = lsn_t;
                // Pace the raft log to the business snapshot point: the
                // manifest frontier is the CommitVec's own (term, index),
                // so the local log's term there always matches.
                if let Err(err) = self.raft.install_snapshot(mudu_raft::TermIndex {
                    term: manifest.raft_term,
                    index: manifest.raft_index,
                }) {
                    warn!(committed_lsn = lsn_t, "raft snapshot install failed: {err}");
                }
            }
            Err(err) => {
                warn!(
                    committed_lsn = lsn_t,
                    "worker snapshot failed (will retry at the next threshold): {err}"
                );
            }
        }
    }

    /// Installs a fetched snapshot serially (the sink fetched and verified
    /// the files): replace storage content, GC the WAL below the snapshot
    /// point, drop the staged stream and re-anchor the chain.
    async fn install_snapshot(
        &mut self,
        manifest: mudu_contract::protocol::ReplSnapshotManifest,
        snapshot_dir: &std::path::Path,
    ) -> RS<()> {
        let resume = manifest.resume_lsn();
        trace!(
            committed_lsn = manifest.committed_lsn,
            "snapshot install: flushing dirty pages"
        );
        // Drain the old dirty pages before the files are replaced: after
        // this, the pre-restore relations hold no pending writes, so the
        // background page-flush loop cannot smear stale pages into the new
        // files while the storage reloads.
        ::worker::worker::flush_worker_dirty_pages(&self.worker).await?;
        trace!("snapshot install: replacing relation files");
        crate::snapshot::install_snapshot_relation_files(
            self.worker.storage().relation_path(),
            snapshot_dir,
            &manifest,
        )
        .await?;
        // Reload the catalog and every relation from the replaced files,
        // then restore the kv dump.
        trace!("snapshot install: reloading meta catalog");
        self.worker.meta_mgr().initialize().await?;
        trace!("snapshot install: reloading relations");
        self.worker.storage().reload_relations_from_disk().await?;
        let kv = crate::snapshot::read_kv_dump(snapshot_dir)?;
        trace!(entries = kv.len(), "snapshot install: restoring kv dump");
        self.worker.storage().restore_kv_latest(kv).await?;
        // WAL: cut any former-leader tail above the snapshot point, reclaim
        // everything below it, and reset the readers.
        trace!("snapshot install: truncating and GC-ing the WAL");
        if self.log.next_lsn().as_u64() > resume {
            self.log.truncate_from_lsn(LSN::new(resume)).await?;
        }
        let (freed, _reclaimed) = self
            .log
            .layout()?
            .gc_prefix_chunks(self.log.fs().as_ref(), resume)
            .await?;
        self.wal_cursor = LogFrameCursor::new(self.log.clone());
        // The staged stream is covered by the snapshot: drop it entirely.
        self.injected.clear();
        self.staging.truncate_from(0).await?;
        // Pending promotes predate the snapshot anchor; new CommitVecs
        // re-drive the chain from here.
        self.pending_targets.clear();
        self.pending_decode.clear();
        self.pending_start = None;
        self.last_promoted = None;
        self.chain = (resume, manifest.seg_hash);
        self.last_snapshot_lsn = manifest.committed_lsn;
        self.state.set_chain(resume, manifest.seg_hash)?;
        self.staging
            .store_committed_point(&PersistedCommittedPoint {
                lsn: resume,
                hash: manifest.seg_hash,
            })?;
        // Advance the raft install frontier to the restored point: the
        // leader then resumes log shipping right above it (term-correct by
        // construction: the manifest frontier is the CommitVec's own
        // (term, index)).
        if manifest.raft_index > 0 {
            trace!(
                raft_index = manifest.raft_index,
                "snapshot install: installing the raft frontier"
            );
            self.raft.install_snapshot(mudu_raft::TermIndex {
                term: manifest.raft_term,
                index: manifest.raft_index,
            })?;
        }
        trace!(
            committed_lsn = manifest.committed_lsn,
            freed_bytes = freed,
            "snapshot installed"
        );
        Ok(())
    }

    /// Promotes the chain to `(lsn_t, hash_t)`; `lsn_t` is the committed
    /// segment's highest frame LSN, so the new resume point is `lsn_t + 1`.
    async fn try_promote(&mut self, lsn_t: u64, hash_t: SegHash) -> PromoteOutcome {
        if lsn_t < self.chain.0 {
            return PromoteOutcome::Done;
        }
        // The backend allocator is the WAL watermark: frames with an LSN
        // below it are present (leader-side produced segments included).
        let mut wal_hi = self.log.next_lsn().as_u64();
        // Former-leader tail truncation: only a non-leader may cut. On a
        // live leader the allocator legitimately runs ahead of the chain
        // (frames are produced before they commit), and cutting would wipe
        // freshly committed content and the queued next segment — exactly
        // the tail this check is meant to discard on a *follower* whose
        // previous leadership left uncommitted frames behind.
        if wal_hi > self.chain.0 && self.share.role() != mudu_raft::RaftRole::Leader {
            // Former leader's uncommitted tail, still uncut: cut it now,
            // before the WAL would be extended past the committed point.
            if let Err(err) = self.log.truncate_from_lsn(LSN::new(self.chain.0)).await {
                return PromoteOutcome::Fatal(err);
            }
            wal_hi = self.log.next_lsn().as_u64();
            self.wal_cursor = LogFrameCursor::new(self.log.clone());
        }
        let frames = match self.collect_promote_frames(lsn_t, wal_hi).await {
            Ok(frames) => frames,
            Err(outcome) => return outcome,
        };
        let hash = seg_hash_chain(&self.chain.1, &frames);
        if hash != hash_t {
            return PromoteOutcome::Fatal(mudu_error!(
                ErrorCode::CorruptedData,
                format!(
                    "committed segment hash mismatch at lsn {}: local chain content diverges from the raft-committed vector (local {} vs committed {}, chain resume {}, {} frames, manual reseed required)",
                    lsn_t,
                    crate::seg_hash::seg_hash_hex(&hash),
                    crate::seg_hash::seg_hash_hex(&hash_t),
                    self.chain.0,
                    frames.len()
                )
            ));
        }
        let frames_count = frames.len();
        let mut new_frames = Vec::new();
        for frame in frames {
            let lsn = frame_lsn(&frame).map(|lsn| lsn.as_u64()).unwrap_or(0);
            if lsn >= wal_hi {
                new_frames.push(frame);
            }
        }
        if let Err(err) = self.append_and_replay(new_frames).await {
            return PromoteOutcome::Fatal(err);
        }
        let resume = lsn_t.saturating_add(1);
        self.chain = (resume, hash_t);
        if let Err(err) = self.state.set_chain(resume, hash_t).and_then(|()| {
            self.staging
                .store_committed_point(&PersistedCommittedPoint {
                    lsn: resume,
                    hash: hash_t,
                })
        }) {
            return PromoteOutcome::Fatal(err);
        }
        trace!(
            lsn = lsn_t,
            hash = %crate::seg_hash::seg_hash_hex(&hash_t),
            frames = frames_count,
            "promoted worker chain to committed point"
        );
        PromoteOutcome::Done
    }

    /// Collects the frames of the committed segment `[chain resume, lsn_t]`:
    /// from the worker WAL below the watermark (leader-side segments and
    /// the committed prefix), from the heal-injected map, or from the
    /// staging store.
    async fn collect_promote_frames(
        &mut self,
        lsn_t: u64,
        wal_hi: u64,
    ) -> Result<Vec<Vec<u8>>, PromoteOutcome> {
        let mut frames: Vec<Vec<u8>> = Vec::new();
        let mut next = self.chain.0;
        while next <= lsn_t {
            let frame = if next < wal_hi {
                // Already in the worker WAL: read it back for the hash chain.
                match self.read_wal_frame(next, wal_hi).await {
                    Ok(frame) => frame,
                    Err(err) => return Err(PromoteOutcome::Fatal(err)),
                }
            } else if let Some(frame) = self.injected.get(&next).cloned() {
                frame
            } else {
                match self.staging.frame_bytes(next).await {
                    Ok(Some(frame)) => frame,
                    Ok(None) => {
                        return Err(PromoteOutcome::NotCovered);
                    }
                    Err(err) => return Err(PromoteOutcome::Fatal(err)),
                }
            };
            frames.push(frame);
            next += 1;
        }
        Ok(frames)
    }

    /// Reads back the worker-WAL frame at exactly `next` through the
    /// cursor, skipping anchor-prefix frames the cursor starts at; `wal_hi`
    /// is the allocator watermark the frame must exist below.
    async fn read_wal_frame(&mut self, next: u64, wal_hi: u64) -> RS<Vec<u8>> {
        loop {
            let Some(frame) = self.wal_cursor.next_frame().await? else {
                return Err(mudu_error!(
                    ErrorCode::InvalidState,
                    format!(
                        "worker log ended at lsn {} below its allocator watermark {}",
                        next, wal_hi
                    )
                ));
            };
            let lsn = frame_lsn(&frame)?.as_u64();
            if lsn < next {
                continue;
            }
            if lsn != next {
                return Err(mudu_error!(
                    ErrorCode::InvalidState,
                    format!(
                        "worker log frame order broken: expected lsn {}, got {}",
                        next, lsn
                    )
                ));
            }
            return Ok(frame);
        }
    }

    /// Appends staged frames to the worker WAL, flushes them durable and
    /// replays the decoded XL batches in order.
    async fn append_and_replay(&mut self, new_frames: Vec<Vec<u8>>) -> RS<()> {
        if new_frames.is_empty() {
            return Ok(());
        }
        self.log.append_frames_async(new_frames.clone()).await?;
        self.log.flush_async().await?;
        let batches = decode_xl_batches_with_pending(
            &new_frames,
            &mut self.pending_decode,
            &mut self.pending_start,
        )?;
        for batch in batches {
            self.worker.replay_log_batch(batch).await?;
        }
        Ok(())
    }

    fn inject_frames(&mut self, frames: Vec<Vec<u8>>) -> RS<()> {
        for frame in frames {
            let lsn = frame_lsn(&frame)?.as_u64();
            self.injected.entry(lsn).or_insert(frame);
        }
        Ok(())
    }

    /// Leader-elect heal: drop every staged frame at or beyond the resume
    /// point (the speculative staged tail).
    async fn discard_staging(&mut self) -> RS<()> {
        let keep = self.chain.0;
        self.injected.retain(|lsn, _| *lsn < self.chain.0);
        self.staging.truncate_from(keep).await
    }
}
