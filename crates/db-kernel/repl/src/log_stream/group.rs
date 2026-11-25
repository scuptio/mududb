//! Group-owned external log durability, quorum admission and leader recovery.

mod membership;
mod snapshot;

use super::state::invalid;
use super::{
    AckRound, AckTracker, DurableAck, DurableStreamLog, GroupSnapshotBundle, GroupState,
    ReplicatedStreamRegistry, StreamFetch, StreamLifecycle, StreamSession, StreamTransport,
};
use mudu::common::result::RS;
use mudu_raft::stream::{MembershipVersion, StreamId, StreamProgress};
use mudu_raft::{AdmissionGuard, ConfNodeValue, RaftHandle, RaftRole, TermIndex};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use tokio::sync::{Mutex, Semaphore};

/// Consensus receipt; business visibility still requires the stream's durable
/// apply frontier. Retrying the same request returns its original prefix.
#[derive(Clone, Debug)]
pub struct StreamCommitReceipt {
    /// External durable prefix referenced by consensus.
    pub progress: StreamProgress,
    /// Authoritative Raft view covering the prefix (may be a later barrier).
    pub raft_frontier: TermIndex,
}

struct StreamSlot {
    log: Arc<DurableStreamLog>,
    owner: Mutex<()>,
    budget: Arc<Semaphore>,
}

/// One shared Raft group for all registered log kinds. Owns per-stream admission
/// and recovery; callers must not mutate its logs/registry through another owner.
/// Registry lifecycle changes require rebinding the complete live adapter set.
pub struct GenericReplicationGroup {
    raft: Arc<RaftHandle>,
    registry: Arc<ReplicatedStreamRegistry>,
    streams: BTreeMap<StreamId, StreamSlot>,
    transport: Arc<dyn StreamTransport>,
    running: AdmissionGuard,
    /// The most recently captured bundle, served to replicas whose stream
    /// prefix fell below the retained frontier. A bundle becomes servable only
    /// after the caller durably publishes it.
    snapshot: Mutex<Option<GroupSnapshotBundle>>,
}

impl GenericReplicationGroup {
    /// Binds every live stream to its external log; no additional Raft instance
    /// is created. Payload storage can be shared with business apply adapters.
    pub async fn open(
        raft: Arc<RaftHandle>,
        registry: Arc<ReplicatedStreamRegistry>,
        logs: Vec<Arc<DurableStreamLog>>,
        transport: Arc<dyn StreamTransport>,
    ) -> RS<Self> {
        if !registry.uses_raft(&raft) {
            return Err(invalid("registry and streams must share one Raft owner"));
        }
        let state = registry.refresh().await?;
        let mut streams = BTreeMap::new();
        for log in logs {
            let id = log.descriptor().stream_id;
            let registered = state
                .streams()
                .get(&id)
                .ok_or_else(|| invalid("unregistered stream log"))?;
            let bytes = usize::try_from(log.descriptor().budget.max_inflight_bytes)
                .map_err(|_| invalid("stream budget overflow"))?;
            if log.identity() != state.identity()
                || registered.lifecycle == StreamLifecycle::Tombstoned
                || registered.descriptor != *log.descriptor()
                || bytes == 0
                || bytes > Semaphore::MAX_PERMITS
                || bytes > u32::MAX as usize
            {
                return Err(invalid("incompatible stream log or admission budget"));
            }
            if streams
                .insert(
                    id,
                    StreamSlot {
                        log,
                        owner: Mutex::new(()),
                        budget: Arc::new(Semaphore::new(bytes)),
                    },
                )
                .is_some()
            {
                return Err(invalid("duplicate stream log"));
            }
        }
        if streams.len()
            != state
                .streams()
                .values()
                .filter(|s| s.lifecycle != StreamLifecycle::Tombstoned)
                .count()
        {
            return Err(invalid("missing live stream log"));
        }
        Ok(Self {
            raft,
            registry,
            streams,
            transport,
            running: AdmissionGuard::new(),
            snapshot: Mutex::new(None),
        })
    }

    /// Changes membership only after checking every resulting voter's complete
    /// stream vector and fencing the proof at protocol admission.
    pub async fn update_membership(
        &self,
        expected: MembershipVersion,
        conf: mudu_raft::ConfValue,
        voters: Vec<u64>,
        members: Vec<u64>,
    ) -> RS<mudu_raft::UpdateConfWaiter> {
        membership::update_membership(self, expected, conf, voters, members).await
    }
}

impl GenericReplicationGroup {
    /// Durably appends external payload, gathers current-session old/new quorum
    /// receipts, then commits only its progress reference. No business apply is
    /// implied by this receipt. The business API must validate its payload
    /// before calling; the group validates durable dependency references.
    /// Saturated streams return an explicit error.
    pub async fn append(
        &self,
        id: StreamId,
        request_id: u128,
        payload: Vec<u8>,
        dependencies: Vec<StreamProgress>,
    ) -> RS<StreamCommitReceipt> {
        let slot = self.slot(id)?;
        let bytes = payload
            .len()
            .checked_add(dependencies.len().saturating_mul(64))
            .and_then(|n| n.checked_add(1024))
            .and_then(|n| u32::try_from(n).ok())
            .ok_or_else(|| invalid("append byte budget overflow"))?;
        let _budget = slot
            .budget
            .clone()
            .try_acquire_many_owned(bytes)
            .map_err(|_| {
                mudu::mudu_error!(
                    mudu::error::ErrorCode::ResourceBusy,
                    "stream admission budget exhausted"
                )
            })?;
        let _owner = slot.owner.lock().await;
        self.ensure_running()?;
        let (term, state) = self.registry.leader_view().await?;
        self.validate_slot(id, &state, true)?;
        self.validate_dependencies(&state, &dependencies).await?;
        let (old, new) = self.raft.conf_async().await?;
        let voters = member_ids(&old, &new);
        self.heal(slot, &state, &voters).await?;
        self.ensure_leader(term)?;
        let (progress, _) = slot.log.append(request_id, payload, dependencies).await?;
        if state
            .committed()
            .get(&id)
            .is_some_and(|p| p.end_lsn >= progress.end_lsn)
        {
            return Ok(StreamCommitReceipt {
                progress,
                raft_frontier: state.frontier(),
            });
        }
        let frontier = self
            .commit_prefix(slot, &state, term, old, new, progress.clone())
            .await?;
        Ok(StreamCommitReceipt {
            progress,
            raft_frontier: frontier,
        })
    }

    /// Durably appends a batch of records to one stream under a single
    /// leader barrier and a single GroupCommit: every record is appended to
    /// the local durable log, the new prefix is pushed to the members once,
    /// and one progress reference covers the whole batch. Structs whose
    /// request id already exists return their original prefix (dedup);
    /// dependencies of every record must point at already-committed
    /// progress (no intra-batch dependency edges). Per-record receipts come
    /// back in request order.
    pub async fn append_batch(
        &self,
        id: StreamId,
        records: Vec<(u128, Vec<u8>, Vec<StreamProgress>)>,
    ) -> RS<Vec<StreamCommitReceipt>> {
        if records.is_empty() || records.len() > 256 {
            return Err(invalid("invalid stream batch size"));
        }
        let slot = self.slot(id)?;
        let bytes = batch_budget_bytes(&records)?;
        let _budget = slot
            .budget
            .clone()
            .try_acquire_many_owned(bytes)
            .map_err(|_| {
                mudu::mudu_error!(
                    mudu::error::ErrorCode::ResourceBusy,
                    "stream admission budget exhausted"
                )
            })?;
        let _owner = slot.owner.lock().await;
        self.ensure_running()?;
        // Fast path: a fully duplicated batch already covered by the
        // committed prefix returns its original receipts without a barrier
        // or a new consensus round.
        if let Some(receipts) = self.committed_batch_receipts(id, slot, &records).await? {
            return Ok(receipts);
        }
        let (term, state) = self.registry.leader_view().await?;
        self.validate_slot(id, &state, true)?;
        for (_, _, dependencies) in &records {
            self.validate_dependencies(&state, dependencies).await?;
        }
        let (old, new) = self.raft.conf_async().await?;
        let voters = member_ids(&old, &new);
        self.heal(slot, &state, &voters).await?;
        self.ensure_leader(term)?;
        let mut progressed = Vec::with_capacity(records.len());
        for (request_id, payload, dependencies) in records {
            progressed.push(slot.log.append(request_id, payload, dependencies).await?.0);
        }
        let Some(final_progress) = progressed.last().cloned() else {
            return Err(invalid("empty stream batch"));
        };
        // The whole batch is already covered by the authoritative committed
        // prefix (a retry of a committed batch): return the dedup receipts
        // without a new consensus round.
        if state
            .committed()
            .get(&id)
            .is_some_and(|p| p.end_lsn >= final_progress.end_lsn)
        {
            let frontier = state.frontier();
            return Ok(progressed
                .into_iter()
                .map(|progress| StreamCommitReceipt {
                    progress,
                    raft_frontier: frontier,
                })
                .collect());
        }
        let frontier = self
            .commit_prefix(slot, &state, term, old, new, final_progress)
            .await?;
        Ok(progressed
            .into_iter()
            .map(|progress| StreamCommitReceipt {
                progress,
                raft_frontier: frontier,
            })
            .collect())
    }

    /// Drives one commit of the stream's final prefix: durable-quorum
    /// certificate over the exact membership pair, then one GroupCommit.
    /// Shared by the single-record and batched append paths.
    ///
    /// The ack collection retries until the quorum holds: a lagging or
    /// briefly unreachable peer must not fail the commit. The round ends on
    /// quorum, on leadership loss, or on the group's shutdown fence.
    async fn commit_prefix(
        &self,
        slot: &StreamSlot,
        state: &GroupState,
        term: u64,
        old: ConfNodeValue,
        new: ConfNodeValue,
        final_progress: StreamProgress,
    ) -> RS<TermIndex> {
        let voters = member_ids(&old, &new);
        let round = AckRound {
            identity: state.identity(),
            registry_version: state.registry_version(),
            leader_term: term,
            membership_version: MembershipVersion::from_conf(&old, &new),
            codec_version: slot.log.descriptor().codec_version,
            digest_version: slot.log.descriptor().digest_version,
            progress: final_progress,
        };
        let mut tracker = AckTracker::new(round.clone(), old, new, self.raft.node_id())?;
        tracker.record_local_durable()?;
        let mut guards = vec![self.running.clone()];
        let mut acked = std::collections::HashSet::new();
        let mut sessions = std::collections::HashMap::new();
        while !tracker.quorum_met() {
            self.ensure_running()?;
            self.ensure_leader(term)?;
            for peer in voters.iter().filter(|p| **p != self.raft.node_id()) {
                // The gate opens at quorum; a hung voter must not stall the
                // commit. Voters past the gate get the prefix off the
                // critical path (`offer_prefix_to_stragglers` after commit).
                if tracker.quorum_met() {
                    break;
                }
                if acked.contains(peer) {
                    continue;
                }
                let peer = *peer;
                let Ok(session) = rpc(self.transport.session(peer)).await else {
                    continue;
                };
                if !session.valid.is_valid() {
                    continue;
                }
                // A replaced session revokes that peer's earlier receipt, so
                // the peer rejoins the pending set of the next pass.
                if sessions.get(&peer) != Some(&session.id) {
                    sessions.insert(peer, session.id);
                    tracker.set_peer_session(peer, session.id)?;
                    acked.remove(&peer);
                }
                if let Ok(Some(ack)) =
                    rpc(self.send_prefix(peer, &session, &round, &slot.log)).await
                {
                    if session.valid.is_valid() && ack.node_id == peer && tracker.record_peer(&ack)
                    {
                        guards.push(session.valid.clone());
                        acked.insert(peer);
                    }
                }
            }
            if !tracker.quorum_met() {
                mudu_sys::task::async_::sleep(std::time::Duration::from_millis(50)).await?;
            }
        }
        self.ensure_leader(term)?;
        let frontier = self
            .registry
            .commit_with_guards(vec![tracker.proof()?], guards)
            .await?;
        // Best-effort, off the critical path: voters the quorum gate passed
        // by still get the prefix (their stores stay push-fresh, and their
        // acks feed the leader's subscriber view).
        self.offer_prefix_to_stragglers(&round, &slot.log, &voters, &acked);
        Ok(frontier)
    }

    /// Offers the just-committed prefix to the voters that did not ack during
    /// the gated pass, detached and best-effort: never blocks the commit path,
    /// and failures stay the follower-side catch-up's concern.
    fn offer_prefix_to_stragglers(
        &self,
        round: &AckRound,
        log: &Arc<DurableStreamLog>,
        voters: &std::collections::BTreeSet<u64>,
        acked: &std::collections::HashSet<u64>,
    ) {
        let pending: Vec<u64> = voters
            .iter()
            .filter(|peer| **peer != self.raft.node_id() && !acked.contains(*peer))
            .copied()
            .collect();
        if pending.is_empty() {
            return;
        }
        let transport = self.transport.clone();
        let log = log.clone();
        let round = round.clone();
        let running = self.running.clone();
        let _ = mudu_sys::task::async_::spawn_task_detached("stream-straggler-offer", async move {
            for peer in pending {
                if !running.is_valid() {
                    return Ok::<(), mudu::error::MuduError>(());
                }
                let Ok(session) = rpc(transport.session(peer)).await else {
                    continue;
                };
                for lsn in 0..=round.progress.end_lsn {
                    if !session.valid.is_valid() || !running.is_valid() {
                        return Ok(());
                    }
                    let Ok(frame) = log.read_frame(lsn).await else {
                        return Ok(());
                    };
                    match rpc(transport.push(peer, &session, &round, frame)).await {
                        Ok(Some(_)) => break,
                        Ok(None) => {}
                        Err(_) => break,
                    }
                }
            }
            Ok(())
        });
    }

    /// Receiver entry point after transport authenticates `from` and `session`.
    /// Verifies authority before and after durable I/O; stale leaders never get
    /// a receipt. Payload bytes are never handed to business apply here.
    pub async fn receive(
        &self,
        from: u64,
        session: &StreamSession,
        round: &AckRound,
        frame: &[u8],
    ) -> RS<Option<DurableAck>> {
        let slot = self.slot(round.progress.stream_id)?;
        let bytes = u32::try_from(frame.len().saturating_add(1024))
            .map_err(|_| invalid("frame budget overflow"))?;
        let _budget = slot
            .budget
            .clone()
            .try_acquire_many_owned(bytes)
            .map_err(|_| {
                mudu::mudu_error!(
                    mudu::error::ErrorCode::ResourceBusy,
                    "stream receive budget exhausted"
                )
            })?;
        if round.progress.stream_generation != slot.log.descriptor().stream_generation
            || wal::log_frame::frame_lsn(frame)?.as_u64() > round.progress.end_lsn
        {
            return Err(invalid("frame lies outside the proposed stream prefix"));
        }
        let _owner = slot.owner.lock().await;
        self.validate_sender(from, session, round).await?;
        let state = self.registry.refresh().await?;
        self.validate_slot(round.progress.stream_id, &state, true)?;
        if round.identity != state.identity()
            || round.registry_version != state.registry_version()
            || round.codec_version != slot.log.descriptor().codec_version
            || round.digest_version != slot.log.descriptor().digest_version
        {
            return Err(invalid("stream frame registry or namespace mismatch"));
        }
        // Discard only speculative data after a validated leader changes it.
        let lsn = wal::log_frame::frame_lsn(frame)?.as_u64();
        if let Some(tail) = slot.log.frontier().await? {
            if lsn <= tail.end_lsn && slot.log.read_frame(lsn).await? != frame {
                if state
                    .committed()
                    .get(&round.progress.stream_id)
                    .is_some_and(|p| lsn <= p.end_lsn)
                {
                    return Err(invalid("peer attempted to overwrite committed data"));
                }
                slot.log
                    .truncate_uncommitted(state.committed().get(&round.progress.stream_id))
                    .await?;
            }
        }
        slot.log.receive(frame).await?;
        self.validate_sender(from, session, round).await?;
        if slot.log.contains(&round.progress).await? {
            Ok(Some(DurableAck {
                node_id: self.raft.node_id(),
                session_id: session.id,
                round: round.clone(),
            }))
        } else {
            Ok(None)
        }
    }

    /// Serves a single durable frame. The transport must authenticate a group
    /// member before invoking this read; scopes are checked again here.
    pub async fn fetch(&self, request: StreamFetch) -> RS<Vec<u8>> {
        let slot = self.slot(request.stream_id)?;
        let state = self.registry.refresh().await?;
        if request.identity != state.identity()
            || request.stream_generation != slot.log.descriptor().stream_generation
        {
            return Err(invalid("foreign stream fetch"));
        }
        slot.log.read_frame(request.lsn).await
    }

    /// Fences new admissions, drains active per-stream operations, then stops
    /// the shared Raft owner. Apply drivers must be stopped by the service first.
    pub async fn shutdown(&self) -> RS<()> {
        self.running.invalidate();
        // Stop consensus first so a pending barrier/proposal cannot hold a
        // stream owner forever while shutdown waits for it.
        let stopped = self.raft.stop_async().await;
        let mut failure = None;
        for slot in self.streams.values() {
            let _owner = slot.owner.lock().await;
            if let Err(error) = slot.log.drain().await {
                failure = Some(error);
            }
        }
        stopped?;
        if let Some(error) = failure {
            Err(error)
        } else {
            Ok(())
        }
    }

    fn slot(&self, id: StreamId) -> RS<&StreamSlot> {
        self.ensure_running()?;
        self.streams
            .get(&id)
            .ok_or_else(|| invalid("unknown stream"))
    }
    fn ensure_running(&self) -> RS<()> {
        if self.running.is_valid() {
            Ok(())
        } else {
            Err(invalid("replication group stopped"))
        }
    }
    fn ensure_leader(&self, term: u64) -> RS<()> {
        self.ensure_running()?;
        let status = self.raft.status();
        if status.term == term && status.role == RaftRole::Leader {
            Ok(())
        } else {
            Err(invalid("stream leader changed"))
        }
    }
    fn validate_slot(&self, id: StreamId, state: &GroupState, active: bool) -> RS<()> {
        let slot = self.slot(id)?;
        let stream = state
            .streams()
            .get(&id)
            .ok_or_else(|| invalid("missing stream registration"))?;
        if stream.descriptor != *slot.log.descriptor()
            || (active && stream.lifecycle != StreamLifecycle::Active)
        {
            return Err(invalid("stream registration changed"));
        }
        Ok(())
    }
    async fn validate_dependencies(
        &self,
        state: &GroupState,
        dependencies: &[StreamProgress],
    ) -> RS<()> {
        for dependency in dependencies {
            let source = state
                .streams()
                .get(&dependency.stream_id)
                .ok_or_else(|| invalid("dependency stream is not registered"))?;
            let committed = state
                .committed()
                .get(&dependency.stream_id)
                .ok_or_else(|| invalid("dependency is not committed"))?;
            if source.lifecycle == StreamLifecycle::Tombstoned
                || source.descriptor.stream_generation != dependency.stream_generation
                || dependency.end_lsn > committed.end_lsn
                || (dependency.end_lsn == committed.end_lsn && dependency != committed)
            {
                return Err(invalid(
                    "dependency disagrees with authoritative committed history",
                ));
            }
            if dependency.end_lsn < committed.end_lsn {
                let log = &self.slot(dependency.stream_id)?.log;
                if !log.contains(committed).await? || !log.contains(dependency).await? {
                    return Err(invalid(
                        "historical dependency is not covered by the authoritative local prefix",
                    ));
                }
            }
        }
        Ok(())
    }

    async fn validate_sender(
        &self,
        from: u64,
        session: &StreamSession,
        round: &AckRound,
    ) -> RS<()> {
        self.ensure_running()?;
        let status = self.raft.status();
        let (old, new) = self.raft.conf_async().await?;
        if !session.valid.is_valid()
            || session.id == 0
            || status.term != round.leader_term
            || status.leader_id != Some(from)
            || !round.membership_version.matches(&old, &new)
        {
            return Err(invalid("stale or unauthenticated stream leader/session"));
        }
        Ok(())
    }
    async fn send_prefix(
        &self,
        peer: u64,
        session: &StreamSession,
        round: &AckRound,
        log: &DurableStreamLog,
    ) -> RS<Option<DurableAck>> {
        for lsn in 0..=round.progress.end_lsn {
            if !session.valid.is_valid() {
                return Err(invalid("stream session replaced"));
            }
            if let Some(ack) = self
                .transport
                .push(peer, session, round, log.read_frame(lsn).await?)
                .await?
            {
                return Ok(Some(ack));
            }
        }
        Ok(None)
    }
}

/// The outcome of one committed-prefix fetch pass.
enum FetchOutcome {
    /// Every frame through the target is durably held locally.
    Complete,
    /// A local frame contests the committed prefix (speculative history).
    Divergent,
    /// No member served a needed frame.
    Unavailable,
}

impl GenericReplicationGroup {
    /// Follower-side catch-up (invariant C4): a RaftFollower that learned a
    /// committed progress reference but lacks the payload fetches the missing
    /// frames from the group's members. Learners and lagging voters converge
    /// here. A divergent local history contests the committed prefix: the
    /// local log is reset and the committed prefix refetched from the quorum
    /// (snapshot repair of large streams remains a separate mechanism).
    /// Returns whether every committed prefix is held.
    pub async fn catch_up_missing(&self) -> RS<bool> {
        let state = self.registry.refresh().await?;
        let (old, new) = self.raft.conf_async().await?;
        let peers = member_ids(&old, &new);
        let mut complete = true;
        for (id, target) in state.committed().clone() {
            let slot = self.slot(id)?;
            if slot.log.contains(&target).await? {
                continue;
            }
            let _owner = slot.owner.lock().await;
            let mut done = false;
            // One reset retry: a contested local history (divergent content
            // at or below the committed frontier) is wiped and refetched.
            for _ in 0..2 {
                match self.fetch_frames(slot, &state, &peers, &target).await? {
                    FetchOutcome::Complete | FetchOutcome::Divergent => {
                        if slot.log.contains(&target).await? {
                            // Drop any speculative tail beyond the committed prefix.
                            slot.log.truncate_uncommitted(Some(&target)).await?;
                            done = true;
                            break;
                        }
                        // The committed prefix is contested locally: reset the
                        // log and refetch from the quorum on the next attempt.
                        slot.log.truncate_uncommitted(None).await?;
                    }
                    FetchOutcome::Unavailable => break,
                }
            }
            if !done {
                complete = false;
            }
        }
        Ok(complete)
    }

    /// Reads the committed records of one stream from `from_lsn` through the
    /// authoritative committed prefix (never speculative data). Returns the
    /// committed frontier and the `(lsn, request_id, progress, payload)`
    /// records in ascending LSN order (empty when `from_lsn` lies past the
    /// committed prefix). Each record carries its own durable prefix digest.
    pub async fn committed_records(
        &self,
        stream_id: StreamId,
        from_lsn: u64,
    ) -> RS<(StreamProgress, Vec<(u64, u128, StreamProgress, Vec<u8>)>)> {
        let slot = self.slot(stream_id)?;
        let state = self.registry.refresh().await?;
        let committed = state
            .committed()
            .get(&stream_id)
            .cloned()
            .ok_or_else(|| invalid("stream has no committed prefix"))?;
        let mut records = Vec::new();
        for lsn in from_lsn..=committed.end_lsn {
            let (request_id, record) = slot.log.record(lsn).await?;
            records.push((lsn, request_id, record.progress, record.payload));
        }
        Ok((committed, records))
    }

    /// Serves one member's durable progress at an exact LSN (the promotion
    /// gate's query target).
    pub async fn peer_progress_at(
        &self,
        stream_id: StreamId,
        lsn: u64,
    ) -> RS<Option<StreamProgress>> {
        let slot = self.slot(stream_id)?;
        slot.log.progress_at(lsn).await
    }

    /// Whether a member durably holds every committed stream prefix exactly
    /// (the promotion gate of the membership design): compare each stream's
    /// committed progress against the member's progress at that exact LSN —
    /// a larger LSN never counts as evidence for different content.
    pub async fn member_caught_up(&self, peer: u64) -> RS<bool> {
        self.ensure_running()?;
        let state = self.registry.refresh().await?;
        let (old, new) = self.raft.conf_async().await?;
        if !member_ids(&old, &new).contains(&peer) {
            return Err(invalid("not a group member"));
        }
        for committed in state.committed().values() {
            if peer == self.raft.node_id() {
                if !self
                    .slot(committed.stream_id)?
                    .log
                    .contains(committed)
                    .await?
                {
                    return Ok(false);
                }
                continue;
            }
            let session = self.transport.session(peer).await?;
            match self
                .transport
                .progress_at(peer, &session, committed.stream_id, committed.end_lsn)
                .await?
            {
                Some(progress) if progress == *committed => {}
                _ => return Ok(false),
            }
        }
        Ok(true)
    }

    async fn heal(&self, slot: &StreamSlot, state: &GroupState, peers: &BTreeSet<u64>) -> RS<()> {
        let target = state.committed().get(&slot.log.descriptor().stream_id);
        if let Some(target) = target {
            if !slot.log.contains(target).await? {
                self.fetch_missing(slot, state, peers, target).await?;
                if !slot.log.contains(target).await? {
                    return Err(invalid(
                        "committed stream digest mismatch; snapshot recovery required",
                    ));
                }
            }
        }
        slot.log.truncate_uncommitted(target).await
    }

    /// Fetches every locally missing frame of the committed prefix from the
    /// group's members, in LSN order. Detects a divergent local frame within
    /// the committed prefix (the caller resets and retries); peer failures
    /// without any success yield `Unavailable`.
    async fn fetch_frames(
        &self,
        slot: &StreamSlot,
        state: &GroupState,
        peers: &BTreeSet<u64>,
        target: &StreamProgress,
    ) -> RS<FetchOutcome> {
        let frontier = slot.log.frontier().await?;
        if let Some(frontier) = &frontier {
            if frontier.end_lsn >= target.end_lsn && !slot.log.contains(target).await? {
                // The local log covers the committed prefix but its content
                // differs: the histories contest each other.
                return Ok(FetchOutcome::Divergent);
            }
        }
        let mut lsn = frontier.map_or(0, |p| p.end_lsn.saturating_add(1));
        while lsn <= target.end_lsn {
            let mut stored = false;
            for peer in peers.iter().filter(|p| **p != self.raft.node_id()) {
                let Ok(session) = rpc(self.transport.session(*peer)).await else {
                    continue;
                };
                let request = StreamFetch {
                    identity: state.identity(),
                    stream_id: target.stream_id,
                    stream_generation: target.stream_generation,
                    lsn,
                };
                let Ok(frame) = rpc(self.transport.fetch(*peer, &session, request)).await else {
                    continue;
                };
                if !session.valid.is_valid() {
                    continue;
                }
                // A different local frame occupies this LSN: the histories
                // diverge inside the committed prefix.
                if let Ok(local) = slot.log.read_frame(lsn).await {
                    if local != frame {
                        return Ok(FetchOutcome::Divergent);
                    }
                    stored = true;
                    break;
                }
                match slot.log.receive(&frame).await {
                    Ok(_) => {
                        stored = true;
                        break;
                    }
                    // Lost the race against a concurrently received frame;
                    // the next iteration re-reads the local content.
                    Err(_) => continue,
                }
            }
            if !stored {
                return Ok(FetchOutcome::Unavailable);
            }
            lsn += 1;
        }
        Ok(FetchOutcome::Complete)
    }
    /// The fast path of a batch append: when every record already exists in
    /// the durable log and is covered by the authoritative committed prefix,
    /// return the original receipts without a new consensus round.
    async fn committed_batch_receipts(
        &self,
        id: StreamId,
        slot: &StreamSlot,
        records: &[(u128, Vec<u8>, Vec<StreamProgress>)],
    ) -> RS<Option<Vec<StreamCommitReceipt>>> {
        let state = self.registry.refresh().await?;
        let mut known = Vec::with_capacity(records.len());
        for (request_id, _, _) in records {
            match slot.log.request_progress(*request_id).await? {
                Some(progress)
                    if state
                        .committed()
                        .get(&id)
                        .is_some_and(|committed| committed.end_lsn >= progress.end_lsn) =>
                {
                    known.push(progress)
                }
                _ => return Ok(None),
            }
        }
        let frontier = state.frontier();
        Ok(Some(
            known
                .into_iter()
                .map(|progress| StreamCommitReceipt {
                    progress,
                    raft_frontier: frontier,
                })
                .collect(),
        ))
    }

    /// Fetches every locally missing frame of the committed prefix from the
    /// group's members, in LSN order (leader-side heal path).
    async fn fetch_missing(
        &self,
        slot: &StreamSlot,
        state: &GroupState,
        peers: &BTreeSet<u64>,
        target: &StreamProgress,
    ) -> RS<()> {
        let start = slot
            .log
            .frontier()
            .await?
            .map_or(0, |p| p.end_lsn.saturating_add(1));
        for lsn in start..=target.end_lsn {
            let mut found = false;
            for peer in peers.iter().filter(|p| **p != self.raft.node_id()) {
                let Ok(session) = rpc(self.transport.session(*peer)).await else {
                    continue;
                };
                let request = StreamFetch {
                    identity: state.identity(),
                    stream_id: target.stream_id,
                    stream_generation: target.stream_generation,
                    lsn,
                };
                if let Ok(frame) = rpc(self.transport.fetch(*peer, &session, request)).await {
                    if session.valid.is_valid() && slot.log.receive(&frame).await.is_ok() {
                        found = true;
                        break;
                    }
                }
            }
            if !found {
                return Err(invalid("committed stream frame unavailable from members"));
            }
        }
        Ok(())
    }
}

/// The admission byte budget of one batch: payload bytes plus per-record
/// framing overhead, saturating-safe.
fn batch_budget_bytes(records: &[(u128, Vec<u8>, Vec<StreamProgress>)]) -> RS<u32> {
    let mut bytes = 0u32;
    for (_, payload, dependencies) in records {
        bytes = bytes
            .checked_add(
                u32::try_from(
                    payload
                        .len()
                        .saturating_add(dependencies.len().saturating_mul(64))
                        .saturating_add(1024),
                )
                .map_err(|_| invalid("batch byte budget overflow"))?,
            )
            .ok_or_else(|| invalid("batch byte budget overflow"))?;
    }
    Ok(bytes)
}

fn member_ids(old: &mudu_raft::ConfNodeValue, new: &mudu_raft::ConfNodeValue) -> BTreeSet<u64> {
    old.node
        .nid_log
        .iter()
        .chain(new.node.nid_log.iter())
        .copied()
        .collect()
}

async fn rpc<T>(future: impl std::future::Future<Output = RS<T>>) -> RS<T> {
    mudu_sys::timeout(std::time::Duration::from_secs(5), future)
        .await
        .ok_or_else(|| {
            mudu::mudu_error!(
                mudu::error::ErrorCode::TimedOut,
                "stream peer I/O deadline exceeded"
            )
        })?
}
