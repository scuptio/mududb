//! The unified control-plane service: one Raft replication group
//! carries only stream progress references (registry management records and
//! `GroupCommit` vectors), while catalog and epoch control records replicate
//! through the two external log streams registered at bootstrap.
//!
//! One service instance runs on every control replica: control voters in any
//! deployment, plus control learners (scale-out data-plane nodes subscribe
//! as learners). The service replaces the old separate MetaGroup/EpochGroup
//! pair; the epoch advance path stays fully persisted per record until the
//! P4 sparse-persistence protocol lands.

/// The directory of the control group's applied frontiers.
async fn frontier_dir(log_dir: &std::path::Path) -> RS<std::path::PathBuf> {
    let dir = log_dir.join("apply");
    let fs = mudu_sys::fs::tokio_fs();
    crate::durable_io::ensure_dir(fs.as_ref(), &dir).await?;
    Ok(dir)
}
use super::epoch_machine::EpochStreamMachine;
use super::machine::{MetaApplier, MetaStreamMachine};
use super::reports::EpochReports;
use super::streams::{
    control_adapter_capabilities, epoch_stream_descriptor, meta_stream_descriptor,
};
use super::EpochCheckpoint;
use crate::log_stream::{
    ApplyOutcome, DurableStreamLog, GenericReplicationGroup, LoggedStreamAdapter,
    ReplicatedStreamRegistry, SharedGroup, StreamAdapter, StreamApplyScheduler, StreamStateMachine,
    StreamTransport,
};
use catalog::contract::control_plane::{CONTROL_STREAM_EPOCH, CONTROL_STREAM_META};
use catalog::contract::epoch_op::{EpochHeartbeat, EpochOp};
use catalog::contract::epoch_oracle::ClusterEpochState;
use catalog::contract::meta_op::MetaOp;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_raft::stream::{GroupIdobject, RegistryOp, StreamId, StreamProgress};
use mudu_raft::{RaftCfg, RaftHandle, RaftMsg, RaftRole, RaftTransport};
use mudu_sys::sync::SMutex;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tracing::{trace, warn};

/// Bootstrap registration request id of the catalog stream (fixed: retries
/// and racing registrants converge on the committed receipt).
const BOOTSTRAP_META_REQUEST_ID: u128 = 1;
/// Bootstrap registration request id of the epoch stream.
const BOOTSTRAP_EPOCH_REQUEST_ID: u128 = 2;
/// Upper bound of bootstrap registration wait during startup.
const BOOTSTRAP_TIMEOUT: Duration = Duration::from_secs(60);
/// Poll interval of bootstrap registration and the apply sweep.
const CONTROL_POLL_INTERVAL: Duration = Duration::from_millis(50);

/// Configuration of one node's control-plane service.
pub struct ControlPlaneCfg {
    /// Cluster node id of this replica.
    pub node_id: u64,
    /// Control-plane voter set (Combined: all nodes; scale-out: the fixed
    /// control members).
    pub voters: Vec<u64>,
    /// Control-plane learner set (scale-out data-plane nodes; empty in the
    /// Combined deployment).
    pub learners: Vec<u64>,
    /// Transaction epoch participants: the data-plane nodes that carry
    /// business partitions and must report their drained epoch before the
    /// stable epoch advances. Decoupled from the control Raft membership
    /// (voters/learners): a dedicated control node is a voter but not an epoch
    /// participant.
    pub epoch_participants: Vec<u64>,
    /// Directory of the control group's raft log, streams and checkpoints.
    pub log_dir: PathBuf,
    /// Raft tick interval in milliseconds.
    pub tick_ms: u64,
    /// Election timeout in ticks.
    pub election_timeout_ticks: u64,
    /// How often the leader evaluates advancing the stable epoch.
    pub advance_interval: Duration,
    /// Namespace and recovery authority of the control group.
    pub identity: GroupIdobject,
    /// The shared cluster epoch state the workers' oracle reads.
    pub epoch_state: Arc<ClusterEpochState>,
    /// The shared serving gate the control plane opens once the local catalog
    /// is synced (A-05). The data plane's write gate reads the same flag.
    pub control_gate: Arc<AtomicBool>,
    /// Enables the sparse epoch protocol: advance the stable/current epoch in
    /// memory within a committed reserved range instead of committing an
    /// `Advance` record per tick (P4). Off by default while the protocol is
    /// rolled out; the committed per-tick `Advance` path stays the default.
    pub sparse_epoch: bool,
    /// Bootstrap stop signal: a server shutdown aborts the startup stream
    /// registration wait promptly instead of waiting out the timeout.
    pub bootstrap_stop: Option<mudu_sys::sync::async_::stop_flag::StopRx>,
}

/// The outcome of a committed-records read served to a data-plane control
/// client.
pub enum CommittedRead {
    /// The client's applied prefix lines up with the committed history;
    /// `records` is the contiguous suffix from its resume point.
    Structs {
        /// The authoritative committed frontier.
        committed: StreamProgress,
        /// Committed `(lsn, request_id, progress, payload)` records in
        /// ascending LSN order.
        records: Vec<(u64, u128, StreamProgress, Vec<u8>)>,
    },
    /// The stream has no committed prefix yet (empty history): the client is
    /// trivially in sync.
    Empty,
    /// The client's namespace or applied prefix does not line up with this
    /// replica's committed history; the client must reset and re-sync from
    /// scratch.
    Resync,
}

/// Shared internals of the control-plane service (the background tasks hold
/// this rather than the service itself).
struct ControlPlaneInner {
    node_id: u64,
    raft: Arc<RaftHandle>,
    registry: Arc<ReplicatedStreamRegistry>,
    group: Arc<GenericReplicationGroup>,
    scheduler: Arc<StreamApplyScheduler>,
    meta: Arc<MetaStreamMachine>,
    epoch: Arc<EpochStreamMachine>,
    reports: SMutex<EpochReports>,
    advance_interval: Duration,
    /// The shared serving gate opened once the catalog is synced (A-05).
    control_gate: Arc<AtomicBool>,
    /// Whether the sparse epoch protocol is enabled.
    sparse_epoch: bool,
    /// Set when the service is stopping; background tasks exit on it and
    /// treat in-flight Raft errors after it as clean shutdown.
    stopping: mudu_sys::tokio::sync::watch::Sender<bool>,
}

/// The control-plane service running on one node.
pub struct ControlPlaneService {
    inner: Arc<ControlPlaneInner>,
    tasks: SMutex<Vec<crate::background::BackgroundTask>>,
}

impl ControlPlaneInner {
    /// Whether the local catalog has applied every committed meta record.
    async fn synced(&self) -> RS<bool> {
        let state = self.registry.refresh().await?;
        let meta_committed = state
            .committed()
            .get(&StreamId(CONTROL_STREAM_META))
            .cloned();
        let meta_applied = self.meta.applied_frontier().await?;
        Ok(meta_applied == meta_committed)
    }
}

impl ControlPlaneService {
    /// Starts the control-plane replica: the shared Raft group, the stream
    /// registry, both control stream logs, the replication group, the apply
    /// driver and the epoch advance ticker.
    pub async fn start_async(
        cfg: ControlPlaneCfg,
        raft_transport: Arc<dyn RaftTransport>,
        stream_transport: Arc<dyn StreamTransport>,
        meta_applier: Arc<dyn MetaApplier>,
    ) -> RS<Arc<Self>> {
        let raft_cfg = RaftCfg::new(
            cfg.node_id,
            cfg.voters.clone(),
            cfg.tick_ms,
            cfg.election_timeout_ticks,
            cfg.log_dir.join("raft"),
        )
        // Per-node election-timeout phasing (see `raft_service`): node ids
        // are 64-bit name hashes, so reduce to a bounded tick phase.
        .with_election_timeout_offset(cfg.node_id % cfg.election_timeout_ticks.max(1))
        .with_learners(cfg.learners.clone());
        let raft = Arc::new(RaftHandle::start_async(raft_cfg, raft_transport).await?);
        let service = Self::assemble(cfg, raft, stream_transport, meta_applier).await?;
        Ok(service)
    }

    /// Assembles the service on an already-started Raft handle (the caller
    /// bootstrapped the group membership itself; used by tests wiring
    /// in-memory networks and by nodes rejoining an existing group).
    pub async fn start_on_raft(
        cfg: ControlPlaneCfg,
        raft: Arc<RaftHandle>,
        stream_transport: Arc<dyn StreamTransport>,
        meta_applier: Arc<dyn MetaApplier>,
    ) -> RS<Arc<Self>> {
        Self::assemble(cfg, raft, stream_transport, meta_applier).await
    }

    /// Assembles the service on an already-opened shared replication group
    /// (S-02): the meta and epoch streams ride the shared registry/group instead
    /// of this service opening its own.
    pub async fn start_on_shared_group(
        cfg: ControlPlaneCfg,
        shared: Arc<SharedGroup>,
        meta_applier: Arc<dyn MetaApplier>,
    ) -> RS<Arc<Self>> {
        let meta_log = shared
            .logs
            .get(&StreamId(CONTROL_STREAM_META))
            .cloned()
            .ok_or_else(|| {
                mudu_error!(
                    ErrorCode::InvalidState,
                    "shared group is missing the meta stream"
                )
            })?;
        let epoch_log = shared
            .logs
            .get(&StreamId(CONTROL_STREAM_EPOCH))
            .cloned()
            .ok_or_else(|| {
                mudu_error!(
                    ErrorCode::InvalidState,
                    "shared group is missing the epoch stream"
                )
            })?;
        Self::assemble_machines(
            cfg,
            shared.raft.clone(),
            shared.registry.clone(),
            shared.group.clone(),
            meta_log,
            epoch_log,
            meta_applier,
            true,
        )
        .await
    }

    async fn assemble(
        cfg: ControlPlaneCfg,
        raft: Arc<RaftHandle>,
        stream_transport: Arc<dyn StreamTransport>,
        meta_applier: Arc<dyn MetaApplier>,
    ) -> RS<Arc<Self>> {
        let registry = Arc::new(
            ReplicatedStreamRegistry::open(
                cfg.identity,
                raft.clone(),
                cfg.log_dir.join("registry.checkpoint"),
                control_adapter_capabilities(),
            )
            .await?,
        );
        ensure_streams_registered(&registry, cfg.bootstrap_stop.clone()).await?;
        let meta_log = Arc::new(
            DurableStreamLog::open(
                cfg.log_dir.join("stream-meta"),
                cfg.identity,
                meta_stream_descriptor(),
            )
            .await?,
        );
        let epoch_log = Arc::new(
            DurableStreamLog::open(
                cfg.log_dir.join("stream-epoch"),
                cfg.identity,
                epoch_stream_descriptor(),
            )
            .await?,
        );
        let group = Arc::new(
            GenericReplicationGroup::open(
                raft.clone(),
                registry.clone(),
                vec![meta_log.clone(), epoch_log.clone()],
                stream_transport,
            )
            .await?,
        );
        Self::assemble_machines(
            cfg,
            raft,
            registry,
            group,
            meta_log,
            epoch_log,
            meta_applier,
            false,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn assemble_machines(
        cfg: ControlPlaneCfg,
        raft: Arc<RaftHandle>,
        registry: Arc<ReplicatedStreamRegistry>,
        group: Arc<GenericReplicationGroup>,
        meta_log: Arc<DurableStreamLog>,
        epoch_log: Arc<DurableStreamLog>,
        meta_applier: Arc<dyn MetaApplier>,
        shared: bool,
    ) -> RS<Arc<Self>> {
        let apply_dir = frontier_dir(&cfg.log_dir).await?;
        let recovery_state = registry.refresh().await?;
        let meta = Arc::new(
            MetaStreamMachine::open(
                &meta_log,
                meta_applier,
                &apply_dir,
                recovery_state
                    .committed()
                    .get(&meta_log.descriptor().stream_id),
            )
            .await?,
        );
        let epoch = Arc::new(
            EpochStreamMachine::open(&epoch_log, &apply_dir, cfg.epoch_state.clone()).await?,
        );
        let adapters: Vec<Arc<dyn StreamAdapter>> = vec![
            Arc::new(LoggedStreamAdapter::new(meta_log, meta.clone())),
            Arc::new(LoggedStreamAdapter::new(epoch_log, epoch.clone())),
        ];
        let state = registry.refresh().await?;
        let scheduler = if shared {
            Arc::new(StreamApplyScheduler::new_partial(&state, adapters)?)
        } else {
            Arc::new(StreamApplyScheduler::new(&state, adapters)?)
        };
        let (stopping, _) = mudu_sys::tokio::sync::watch::channel(false);
        cfg.control_gate.store(false, Ordering::Relaxed);
        let inner = Arc::new(ControlPlaneInner {
            node_id: cfg.node_id,
            raft,
            registry,
            group,
            scheduler,
            meta,
            epoch,
            reports: SMutex::new(EpochReports::new(cfg.epoch_participants.clone())),
            advance_interval: cfg.advance_interval,
            control_gate: cfg.control_gate.clone(),
            sparse_epoch: cfg.sparse_epoch,
            stopping,
        });
        let mut tasks = Vec::new();
        let driver_inner = inner.clone();
        tasks.push(crate::background::spawn("control-apply", async move {
            let _exit = driver_inner.meta.failure_guard();
            run_apply_driver(driver_inner.clone()).await
        })?);
        let ticker_inner = inner.clone();
        tasks.push(crate::background::spawn(
            "control-epoch-tick",
            async move { run_epoch_ticker(ticker_inner).await },
        )?);
        if inner.sparse_epoch {
            let checkpointer_inner = inner.clone();
            tasks.push(crate::background::spawn(
                "control-epoch-checkpoint",
                async move { run_epoch_checkpointer(checkpointer_inner).await },
            )?);
        }
        Ok(Arc::new(Self {
            inner,
            tasks: SMutex::new(tasks),
        }))
    }

    /// The shared raft handle (the endpoint handler delivers frames here).
    pub fn raft(&self) -> &Arc<RaftHandle> {
        &self.inner.raft
    }

    /// The cluster node id of this replica.
    pub fn node_id(&self) -> u64 {
        self.inner.node_id
    }

    /// Whether this node is the control-plane leader.
    pub fn is_leader(&self) -> bool {
        self.inner.raft.status().role == RaftRole::Leader
    }

    /// The current control-plane leader, when known.
    pub fn leader(&self) -> Option<u64> {
        self.inner.raft.status().leader_id
    }

    /// The current consensus term of this replica.
    pub fn term(&self) -> u64 {
        self.inner.raft.status().term
    }

    /// The shared cluster epoch state the workers' oracle reads.
    pub fn epoch_state(&self) -> &Arc<ClusterEpochState> {
        &self.inner.epoch.state
    }

    /// Delivers an inbound raft frame of the control group.
    pub fn deliver_raft(&self, from: u64, msg: RaftMsg) -> RS<()> {
        self.inner.raft.deliver(from, msg)
    }

    /// Leader-side: submits one catalog operation to the meta stream and
    /// waits until the group commits it. Local apply follows asynchronously;
    /// pair with [`ControlPlaneService::wait_meta_result`] for the
    /// applied-locally result.
    pub async fn submit_meta(&self, request_id: u128, op: MetaOp) -> RS<()> {
        if *self.inner.stopping.borrow() {
            return Err(mudu_error!(
                ErrorCode::InvalidState,
                "control service is stopping"
            ));
        }
        self.inner.meta.ensure_available()?;
        let payload = super::machine::encode_meta_op(&op)?;
        self.inner
            .group
            .append(
                StreamId(CONTROL_STREAM_META),
                request_id,
                payload,
                Vec::new(),
            )
            .await?;
        Ok(())
    }

    /// Waits until the request's record is applied by this node's local
    /// catalog. A retry of an already-committed request resolves through the
    /// rebuilt request index instead of re-applying, so the result survives
    /// leader failover and restarts. Pending results fail promptly if local
    /// apply fails, its driver exits, or the service stops; this does not imply
    /// the operation was uncommitted or rolled back.
    pub async fn wait_meta_result(&self, request_id: u128, timeout: Duration) -> RS<()> {
        let machine = self.inner.meta.clone();
        let wait = machine.wait_request(request_id, self.inner.stopping.subscribe());
        mudu_sys::timeout(timeout, wait).await.ok_or_else(|| {
            mudu_error!(
                ErrorCode::TimedOut,
                "control meta op apply timed out (commit or local apply)"
            )
        })?
    }

    /// Fences a data-plane node: commits an `EpochOp::Fence` through the epoch
    /// stream, so every replica drops the node from the epoch participant set
    /// and the stable epoch may advance without its drain report. The fencing
    /// decision must come from a durable authority change (e.g. the data
    /// group's re-election or a membership removal) — never from heartbeat
    /// timeout alone. Idempotent per node.
    pub async fn fence_node(&self, node_id: u64) -> RS<()> {
        if *self.inner.stopping.borrow() {
            return Err(mudu_error!(
                ErrorCode::InvalidState,
                "control service is stopping"
            ));
        }
        if !self.is_leader() {
            return Err(mudu_error!(
                ErrorCode::InvalidState,
                "fence must be committed by the control leader"
            ));
        }
        let payload = super::machine::encode_epoch_op(&EpochOp::Fence { node_id })?;
        self.inner
            .group
            .append(
                StreamId(CONTROL_STREAM_EPOCH),
                u128::from(node_id),
                payload,
                Vec::new(),
            )
            .await?;
        Ok(())
    }

    /// Whether this replica has applied every committed catalog record (the
    /// A-05 service barrier). A node must serve only after this returns true:
    /// serving before the catalog sync exposes a stale local catalog. The epoch
    /// stream advances continuously through the shared oracle state and is not
    /// part of this one-shot startup gate.
    pub async fn is_synced(&self) -> RS<bool> {
        self.inner.synced().await
    }

    /// Waits until the catalog is fully applied, or the timeout elapses (the
    /// A-05 service barrier).
    pub async fn wait_synced(&self, timeout: Duration) -> RS<()> {
        mudu_sys::timeout(timeout, self.wait_synced_inner())
            .await
            .ok_or_else(|| mudu_error!(ErrorCode::TimedOut, "control plane did not sync in time"))?
    }

    async fn wait_synced_inner(&self) -> RS<()> {
        while !self.is_synced().await? {
            mudu_sys::task::async_::sleep(CONTROL_POLL_INTERVAL).await?;
        }
        Ok(())
    }

    /// Serves one member's durable progress at an exact LSN (the promotion
    /// gate's query target; delegates to the group).
    pub async fn peer_progress_at(
        &self,
        stream_id: StreamId,
        lsn: u64,
    ) -> RS<Option<mudu_raft::stream::StreamProgress>> {
        self.inner.group.peer_progress_at(stream_id, lsn).await
    }

    /// The committed frontier of every live control stream, served to
    /// data-plane control clients so they can converge without joining the
    /// control Raft group.
    pub async fn committed_frontiers(&self) -> RS<Vec<StreamProgress>> {
        let state = self.inner.registry.refresh().await?;
        Ok(state.committed().values().cloned().collect())
    }

    /// The current in-memory epoch checkpoint (stable/current/reserved_upper),
    /// served to control clients. Under the sparse epoch protocol the
    /// stable/current edges may run ahead of the committed epoch records,
    /// bounded by the committed reserved range.
    pub fn epoch_checkpoint(&self) -> RS<EpochCheckpoint> {
        self.inner.epoch.checkpoint_state()
    }

    /// Whether the given identity names this control group.
    pub async fn matches_identity(&self, identity: &GroupIdobject) -> RS<bool> {
        let state = self.inner.registry.refresh().await?;
        Ok(&state.identity() == identity)
    }

    /// Reads the committed records of one control stream from `from_lsn`,
    /// after verifying `identity` names this group and `applied` is an exact
    /// prefix of the committed history (digest-compared): a client can never
    /// skip updates or apply a stale version after a disconnect, restart or
    /// leader switch. A client whose `applied` prefix does not line up with
    /// the committed history gets [`CommittedRead::Resync`] instead of a
    /// partial answer.
    pub async fn committed_records(
        &self,
        identity: &GroupIdobject,
        stream_id: StreamId,
        from_lsn: u64,
        applied: Option<&StreamProgress>,
    ) -> RS<CommittedRead> {
        let state = self.inner.registry.refresh().await?;
        if &state.identity() != identity {
            return Ok(CommittedRead::Resync);
        }
        let Some(committed) = state.committed().get(&stream_id).cloned() else {
            return Ok(CommittedRead::Empty);
        };
        if let Some(applied) = applied {
            if applied.stream_id != committed.stream_id
                || applied.stream_generation != committed.stream_generation
                || applied.end_lsn > committed.end_lsn
            {
                return Ok(CommittedRead::Resync);
            }
            let local = self
                .inner
                .group
                .peer_progress_at(stream_id, applied.end_lsn)
                .await?;
            if local.as_ref() != Some(applied) {
                return Ok(CommittedRead::Resync);
            }
        }
        let start = applied.map_or(from_lsn, |prefix| prefix.end_lsn.saturating_add(1));
        let (committed, records) = self.inner.group.committed_records(stream_id, start).await?;
        Ok(CommittedRead::Structs { committed, records })
    }

    /// Structs a closed-prefix report only for the current control leader term.
    pub fn note_drained(&self, report: EpochHeartbeat) {
        let status = self.inner.raft.status();
        if status.role != RaftRole::Leader {
            return;
        }
        if let Ok(mut reports) = self.inner.reports.lock() {
            reports.note(status.term, report);
        }
    }

    /// Receiver entry of a pushed stream frame (delegates to the group).
    pub async fn receive_stream_frame(
        &self,
        from: u64,
        session: &crate::log_stream::StreamSession,
        round: &crate::log_stream::AckRound,
        frame: &[u8],
    ) -> RS<Option<crate::log_stream::DurableAck>> {
        self.inner.group.receive(from, session, round, frame).await
    }

    /// Serves one durable frame to a group member (delegates to the group).
    pub async fn fetch_stream_frame(&self, request: crate::log_stream::StreamFetch) -> RS<Vec<u8>> {
        self.inner.group.fetch(request).await
    }

    /// Stops the service: signals the background tasks first so they stop
    /// starting new work, then stops consensus (which resolves every
    /// in-flight proposal waiter, unblocking detached appends and any task
    /// waiting on a stream's owner), and only then joins the tasks. Must be
    /// called from outside any async executor.
    pub fn stop(&self) -> RS<()> {
        if mudu_sys::task::async_::has_tokio_runtime() {
            return Err(mudu_error!(
                ErrorCode::InvalidState,
                "control service stop must run outside the async executor"
            ));
        }
        self.inner.stopping.send_replace(true);
        let runtime = mudu_sys::task::async_::build_current_thread_runtime()?;
        let group = self.inner.group.clone();
        // Stopping the raft driver resolves pending proposal waiters with a
        // stopped error, so a detached in-flight append (e.g. of a
        // partitioned leader whose commit can never arrive) unwinds and
        // releases its stream owner before the group drains the logs.
        let stop_result = runtime.block_on(async move { group.shutdown().await });
        let tasks = std::mem::take(&mut *self.tasks.lock()?);
        let mut task_result = Ok(());
        for task in tasks {
            task_result = task_result.and(task.join());
        }
        stop_result.and(task_result)
    }
}

/// Registers the two control streams; any node that becomes leader commits
/// the fixed registrations, and followers converge through the replicated
/// registry. Receipts make concurrent/duplicate registration idempotent.
/// `bootstrap_stop` aborts the wait promptly on server shutdown instead of
/// waiting out the timeout.
async fn ensure_streams_registered(
    registry: &Arc<ReplicatedStreamRegistry>,
    mut bootstrap_stop: Option<mudu_sys::sync::async_::stop_flag::StopRx>,
) -> RS<()> {
    let wait = async {
        loop {
            let state = registry.refresh().await?;
            let meta_registered = state.streams().contains_key(&StreamId(CONTROL_STREAM_META));
            let epoch_registered = state
                .streams()
                .contains_key(&StreamId(CONTROL_STREAM_EPOCH));
            if meta_registered && epoch_registered {
                return Ok(());
            }
            if registry.leader_view().await.is_ok() {
                if !meta_registered {
                    let _ = registry
                        .change(
                            BOOTSTRAP_META_REQUEST_ID,
                            RegistryOp::Register(meta_stream_descriptor()),
                        )
                        .await;
                }
                if !epoch_registered {
                    let _ = registry
                        .change(
                            BOOTSTRAP_EPOCH_REQUEST_ID,
                            RegistryOp::Register(epoch_stream_descriptor()),
                        )
                        .await;
                }
            }
            if let Some(stop) = &mut bootstrap_stop {
                mudu_sys::tokio::select! {
                    _ = mudu_sys::task::async_::sleep(CONTROL_POLL_INTERVAL) => {}
                    _ = stop.changed() => {
                        return Err(mudu_error!(
                            ErrorCode::Interrupted,
                            "control stream registration aborted by shutdown"
                        ));
                    }
                }
            } else {
                mudu_sys::task::async_::sleep(CONTROL_POLL_INTERVAL).await?;
            }
        }
    };
    mudu_sys::timeout(BOOTSTRAP_TIMEOUT, wait)
        .await
        .ok_or_else(|| {
            mudu_error!(
                ErrorCode::TimedOut,
                "control stream registration did not complete in time"
            )
        })?
}

/// The apply driver: on a periodic sweep, applies every newly committed
/// stream record until no stream progresses. A stalled stream never blocks an
/// independent stream (per-stream outcomes). It deliberately does NOT
/// subscribe to per-commit notifications: the commit watch channel treats a
/// lagging subscriber as fatal to the Raft driver, and apply rounds do
/// unbounded storage I/O, so a loaded group could otherwise lose its Raft
/// driver to backpressure.
async fn run_apply_driver(inner: Arc<ControlPlaneInner>) -> RS<()> {
    let mut stop = inner.stopping.subscribe();
    let mut sweep = mudu_sys::tokio::time::interval(CONTROL_POLL_INTERVAL);
    sweep.set_missed_tick_behavior(mudu_sys::tokio::time::MissedTickBehavior::Skip);
    loop {
        mudu_sys::tokio::select! {
            _ = sweep.tick() => {}
            _ = stop.changed() => return Ok(()),
        }
        loop {
            // Fetch any locally missing committed payload before applying
            // (invariant C4: progress is never applied without its bytes).
            match inner.group.catch_up_missing().await {
                Ok(_) => {}
                Err(error) if *stop.borrow() => {
                    trace!("control apply driver exits during shutdown: {}", error);
                    return Ok(());
                }
                Err(error) => {
                    warn!("control stream catch-up failed: {}", error);
                    break;
                }
            }
            let state = match inner.registry.refresh().await {
                Ok(state) => state,
                // A Raft error while stopping is a clean shutdown.
                Err(error) if *stop.borrow() => {
                    trace!("control apply driver exits during shutdown: {}", error);
                    return Ok(());
                }
                Err(error) => return Err(error),
            };
            let outcomes = inner.scheduler.apply_round(&state).await?;
            let mut progressed = false;
            for (stream, outcome) in outcomes {
                match outcome {
                    Ok(ApplyOutcome::Applied(_)) => progressed = true,
                    Ok(_) => {}
                    Err(error) => {
                        warn!(?stream, "control stream apply failed: {}", error);
                    }
                }
            }
            if !progressed {
                break;
            }
        }
        // Reconcile the epoch participant set with the durable fenced set: a
        // fenced node is dropped from the participants, so the stable epoch may
        // advance without its drain report. Idempotent, and replayed on every
        // replica after a leader switch.
        if let Ok(checkpoint) = inner.epoch.checkpoint_state() {
            if let Ok(mut reports) = inner.reports.lock() {
                reports.reconcile(&checkpoint.fenced);
            }
        }
        // The serving gate follows the catalog sync: opened once the meta
        // stream has applied every committed record, closed again if a new
        // record commits ahead of local apply.
        let synced = match inner.synced().await {
            Ok(synced) => synced,
            Err(error) if *stop.borrow() => {
                trace!("control apply driver exits during shutdown: {}", error);
                return Ok(());
            }
            Err(error) => return Err(error),
        };
        inner.control_gate.store(synced, Ordering::Relaxed);
    }
}

/// Leader-side epoch ticker: aggregates drained-epoch reports and appends
/// one advance record to the epoch stream per step. The request id derives
/// from the advance itself, so recomputing an identical advance dedups
/// against the durable log instead of growing it.
async fn run_epoch_ticker(inner: Arc<ControlPlaneInner>) -> RS<()> {
    let mut status = inner.raft.watch_status_async()?;
    let mut stop = inner.stopping.subscribe();
    let mut ticks =
        mudu_sys::tokio::time::interval(inner.advance_interval.max(Duration::from_millis(1)));
    ticks.set_missed_tick_behavior(mudu_sys::tokio::time::MissedTickBehavior::Skip);
    ticks.tick().await;
    // At most one in-flight advance: on a partitioned leader an append can
    // wait on a commit that never arrives, and detached appends would pile
    // up on the stream's owner otherwise.
    let mut appending = None;
    loop {
        if let Some(handle) = &mut appending {
            mudu_sys::tokio::select! {
                result = handle => {
                    match result {
                        Ok(Some(Ok(_receipt))) => {}
                        Ok(Some(Err(error))) => {
                            if *stop.borrow() { return Ok(()); }
                            trace!("epoch advance append failed: {}", error);
                        }
                        Ok(None) => return Ok(()),
                        Err(error) => {
                            if *stop.borrow() { return Ok(()); }
                            trace!("epoch advance append task failed: {}", error);
                        }
                    }
                    appending = None;
                }
                _ = stop.changed() => return Ok(()),
            }
            continue;
        }
        mudu_sys::tokio::select! {
            _ = ticks.tick() => {},
            changed = status.changed() => {
                if changed.is_err() { return Ok(()); }
                continue;
            }
            _ = stop.changed() => return Ok(()),
        }
        if inner.raft.status().role != RaftRole::Leader {
            continue;
        }
        let Some((request_id, payload)) = epoch_step(&inner).await? else {
            continue;
        };
        // Run the append detached: the durable log's mutation must run to
        // completion (dropping the future mid-persist would desync the
        // in-memory index from the staged bytes). Shutdown never joins on
        // it — the group's shutdown stops consensus first, which resolves
        // the append's waiters.
        let group = inner.group.clone();
        appending = Some(mudu_sys::task::async_::spawn_task_detached(
            "control-epoch-append",
            async move {
                group
                    .append(
                        StreamId(CONTROL_STREAM_EPOCH),
                        request_id,
                        payload,
                        Vec::new(),
                    )
                    .await
            },
        )?);
    }
}

/// The next epoch operation the leader should perform. Returns the append to
/// commit (a sparse reserve or a per-tick advance), or performs the sparse
/// in-memory advance and returns `None`, or `None` when nothing to do.
async fn epoch_step(inner: &ControlPlaneInner) -> RS<Option<(u128, Vec<u8>)>> {
    if inner.sparse_epoch {
        // Sparse protocol (P4): replenish the reserve when near exhaustion,
        // then advance the stable/current epoch in memory within the range.
        if let Some(reserved_upper) = next_epoch_reserve(inner) {
            let payload = super::machine::encode_epoch_op(&EpochOp::Reserve { reserved_upper })?;
            return Ok(Some((reserved_upper as u128, payload)));
        }
        if let Some(advance) = next_epoch_advance(inner) {
            inner
                .epoch
                .advance_in_memory(advance.0, advance.1, advance.2)
                .await?;
        }
        return Ok(None);
    }
    let Some(advance) = next_epoch_advance(inner) else {
        return Ok(None);
    };
    let request_id = ((advance.0 as u128) << 64) | (advance.1 as u128);
    let payload = super::machine::encode_epoch_op(&EpochOp::Advance {
        stable: advance.0,
        current: advance.1,
        physical_millis: advance.2,
    })?;
    Ok(Some((request_id, payload)))
}

/// Computes the next `(stable, current, physical_millis)` advance from the
/// aggregated reports, or `None` when no advance is needed.
fn next_epoch_advance(inner: &ControlPlaneInner) -> Option<(u64, u64, u64)> {
    let min_drained = inner
        .reports
        .lock()
        .ok()?
        .minimum(inner.raft.status().term)?;
    let state = &inner.epoch.state;
    let current_stable = state.stable.load(Ordering::Acquire);
    let live = state.current.load(Ordering::Acquire);
    // The barrier advances to the smallest drained epoch over all reporting
    // nodes. A drained node attests the live edge, so the barrier can cross
    // it once every participant has drained; the epoch only then advances to
    // live + 1, so a new registration never joins an epoch the barrier
    // already crossed.
    let new_stable = min_drained;
    let someone_busy = min_drained < live;
    if new_stable <= current_stable && !someone_busy {
        return None;
    }
    Some((
        new_stable.max(current_stable),
        live.checked_add(1)?,
        mudu_sys::time::system_time_now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
            .min(u64::MAX as u128) as u64,
    ))
}

/// How close to the reserved upper the live edge must be before the leader
/// commits a fresh reserve (sparse epoch protocol).
const EPOCH_RESERVE_MARGIN: u64 = 100_000;
/// How many epochs each committed reserve grants.
const EPOCH_RESERVE_STEP: u64 = 1_000_000;

/// The candidate interval between sparse epoch checkpoints (E-07).
const EPOCH_CHECKPOINT_INTERVAL: Duration = Duration::from_secs(60);

/// Periodically persists the in-memory epoch checkpoint and does a final
/// checkpoint on shutdown, so the sparse in-memory advance survives a restart.
async fn run_epoch_checkpointer(inner: Arc<ControlPlaneInner>) -> RS<()> {
    let mut stop = inner.stopping.subscribe();
    let mut interval = mudu_sys::tokio::time::interval(EPOCH_CHECKPOINT_INTERVAL);
    interval.set_missed_tick_behavior(mudu_sys::tokio::time::MissedTickBehavior::Skip);
    interval.tick().await;
    loop {
        mudu_sys::tokio::select! {
            _ = interval.tick() => {}
            _ = stop.changed() => {
                let _ = inner.epoch.checkpoint().await;
                return Ok(());
            }
        }
        if let Err(error) = inner.epoch.checkpoint().await {
            warn!("epoch checkpoint failed: {}", error);
        }
    }
}

/// The next reserved upper the leader should commit, or `None` when the live
/// edge is far enough from exhaustion.
fn next_epoch_reserve(inner: &ControlPlaneInner) -> Option<u64> {
    let checkpoint = inner.epoch.checkpoint_state().ok()?;
    if checkpoint.reserve_needed(EPOCH_RESERVE_MARGIN) {
        Some(checkpoint.reserved_upper.saturating_add(EPOCH_RESERVE_STEP))
    } else {
        None
    }
}
