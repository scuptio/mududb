//! The data-plane service of one node group (G-03): the group-level
//! assembly binding the per-worker data streams and the `tx_decision`
//! stream to the node group's shared Raft group through the generic
//! log-stream mechanism.
//!
//! Two-phase lifecycle: [`DataPlaneService::open_group`] runs before the
//! workers spawn (registry, streams, logs, the replication group); each
//! worker then installs its machine and commit driver as it starts
//! ([`DataPlaneService::install_worker`]); when every worker is in, the
//! apply driver and the convergence watcher start. The write gate
//! (`serving`) opens only after the leader converges (barrier + catch-up +
//! speculative-tail truncation).

use super::codec::DataSegment;
use super::machine::{DataStreamMachine, TX_DECISION_STREAM};
use super::tx_machine::TxDecisionMachine;
use super::{data_stream_id, DATA_STREAM_KIND, TX_DECISION_KIND};
use crate::background;
use crate::log_stream::{
    AdapterCapabilities, ApplyOutcome, DurableStreamLog, GenericReplicationGroup,
    LoggedStreamAdapter, ReplicatedStreamRegistry, SharedGroup, StreamAdapter,
    StreamApplyScheduler, StreamCommitReceipt, StreamTransport,
};
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_raft::stream::{GroupIdobject, RegistryOp, StreamId};
use mudu_raft::{RaftHandle, RaftRole};
use mudu_sys::sync::SMutex;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tracing::{trace, warn};
use worker::worker::WorkerRuntime;

/// The maximum bytes of one data segment record (frames fit one stream frame).
const DATA_SEGMENT_MAX_RECORD_BYTES: u32 = 1024 * 1024;
/// Outstanding payload byte budget of one worker's data stream.
const DATA_STREAM_INFLIGHT_BYTES: u64 = 128 * 1024 * 1024;
/// The tx_decision stream's record cap.
const TX_DECISION_MAX_RECORD_BYTES: u32 = 64 * 1024;

/// Configuration of one node's data-plane service.
pub struct DataPlaneCfg {
    /// This node's id.
    pub node_id: u64,
    /// The data group's namespace (cluster id, topology group id, generation).
    pub identity: GroupIdobject,
    /// Workers per node.
    pub worker_count: u32,
    /// Directory for the registry checkpoint and the stream logs.
    pub log_dir: std::path::PathBuf,
    /// Committed WAL bytes beyond the last snapshot that trigger the next.
    pub snapshot_threshold_bytes: u64,
    /// The node's shared raft state (the write gate and observability feed).
    pub share: Arc<crate::raft_service::RaftShare>,
    /// Bootstrap stop signal: a server shutdown aborts the startup stream
    /// registration wait promptly instead of waiting out the timeout.
    pub bootstrap_stop: Option<mudu_sys::sync::async_::stop_flag::StopRx>,
}

/// The data-plane service running on one node.
pub struct DataPlaneService {
    worker_count: u32,
    snapshot_threshold_bytes: u64,
    share: Arc<crate::raft_service::RaftShare>,
    apply_dir: std::path::PathBuf,
    pub(super) raft: Arc<RaftHandle>,
    pub(super) registry: Arc<ReplicatedStreamRegistry>,
    pub(super) group: Arc<GenericReplicationGroup>,
    pub(super) logs: BTreeMap<StreamId, Arc<DurableStreamLog>>,
    serving: Arc<AtomicBool>,
    machines: SMutex<BTreeMap<u32, Arc<DataStreamMachine>>>,
    /// In-flight worker WAL read floors (worker index -> lowest LSN the commit
    /// driver still needs). `gc_watermark` clamps to it.
    wal_read_floors: SMutex<BTreeMap<u32, u64>>,
    pub(super) tx_machine: Arc<TxDecisionMachine>,
    pub(super) coordinators:
        SMutex<BTreeMap<catalog::contract::tx_decision::CoordinatorRef, std::sync::Weak<()>>>,
    pub(super) scheduler: SMutex<Option<Arc<StreamApplyScheduler>>>,
    tasks: SMutex<Vec<background::BackgroundTask>>,
    convergence_notify: mudu_sys::tokio::sync::Notify,
    pub(super) stopping: mudu_sys::tokio::sync::watch::Sender<bool>,
    /// Whether this service rides a shared group (S-02): its apply scheduler
    /// owns only the data/tx_decision subset of the registry.
    shared: bool,
}

impl DataPlaneService {
    /// Phase A: opens the group-level pieces (registry, streams, logs, the
    /// replication group) on the node group's existing Raft handle. No
    /// worker machine exists yet; the group can already receive frames.
    pub async fn open_group(
        cfg: DataPlaneCfg,
        raft: Arc<RaftHandle>,
        stream_transport: Arc<dyn StreamTransport>,
    ) -> RS<Arc<Self>> {
        let (stopping, _) = mudu_sys::tokio::sync::watch::channel(false);
        // The registry checkpoint's parent must exist before it opens.
        let fs = mudu_sys::fs::tokio_fs();
        crate::durable_io::ensure_dir(fs.as_ref(), &cfg.log_dir).await?;
        let registry = Arc::new(
            ReplicatedStreamRegistry::open(
                cfg.identity,
                raft.clone(),
                cfg.log_dir.join("registry.checkpoint"),
                data_adapter_capabilities(cfg.worker_count),
            )
            .await?,
        );
        ensure_data_streams_registered(&registry, cfg.worker_count, cfg.bootstrap_stop.clone())
            .await?;
        let mut logs = BTreeMap::new();
        for worker_index in 0..cfg.worker_count {
            let stream_id = data_stream_id(worker_index);
            logs.insert(
                stream_id,
                Arc::new(
                    DurableStreamLog::open(
                        cfg.log_dir.join(format!("stream-data-{worker_index}")),
                        cfg.identity,
                        data_stream_descriptor(stream_id),
                    )
                    .await?,
                ),
            );
        }
        logs.insert(
            TX_DECISION_STREAM,
            Arc::new(
                DurableStreamLog::open(
                    cfg.log_dir.join("stream-tx-decision"),
                    cfg.identity,
                    tx_decision_descriptor(),
                )
                .await?,
            ),
        );
        let log_vec: Vec<_> = logs.values().cloned().collect();
        let group = Arc::new(
            GenericReplicationGroup::open(
                raft.clone(),
                registry.clone(),
                log_vec,
                stream_transport,
            )
            .await?,
        );
        Self::open_machines(cfg, raft, registry, group, logs, stopping, false).await
    }

    /// Phase A over an already-opened shared replication group (S-02): the
    /// data and tx_decision streams ride the shared registry/group instead of
    /// this service opening its own.
    pub async fn open_on_shared_group(
        cfg: DataPlaneCfg,
        shared: Arc<SharedGroup>,
    ) -> RS<Arc<Self>> {
        let (stopping, _) = mudu_sys::tokio::sync::watch::channel(false);
        let mut logs = BTreeMap::new();
        for worker_index in 0..cfg.worker_count {
            let stream_id = data_stream_id(worker_index);
            logs.insert(
                stream_id,
                shared.logs.get(&stream_id).cloned().ok_or_else(|| {
                    mudu_error!(
                        ErrorCode::InvalidState,
                        "shared group is missing a data stream"
                    )
                })?,
            );
        }
        logs.insert(
            TX_DECISION_STREAM,
            shared
                .logs
                .get(&TX_DECISION_STREAM)
                .cloned()
                .ok_or_else(|| {
                    mudu_error!(
                        ErrorCode::InvalidState,
                        "shared group is missing the tx_decision stream"
                    )
                })?,
        );
        Self::open_machines(
            cfg,
            shared.raft.clone(),
            shared.registry.clone(),
            shared.group.clone(),
            logs,
            stopping,
            true,
        )
        .await
    }

    async fn open_machines(
        cfg: DataPlaneCfg,
        raft: Arc<RaftHandle>,
        registry: Arc<ReplicatedStreamRegistry>,
        group: Arc<GenericReplicationGroup>,
        logs: BTreeMap<StreamId, Arc<DurableStreamLog>>,
        stopping: mudu_sys::tokio::sync::watch::Sender<bool>,
        shared: bool,
    ) -> RS<Arc<Self>> {
        let apply_dir = cfg.log_dir.join("apply");
        let fs = mudu_sys::fs::tokio_fs();
        crate::durable_io::ensure_dir(fs.as_ref(), &apply_dir).await?;
        let tx_machine =
            Arc::new(TxDecisionMachine::open(&logs[&TX_DECISION_STREAM], &apply_dir).await?);
        Ok(Arc::new(Self {
            worker_count: cfg.worker_count,
            snapshot_threshold_bytes: cfg.snapshot_threshold_bytes,
            share: cfg.share,
            apply_dir,
            raft,
            registry,
            group,
            logs,
            serving: Arc::new(AtomicBool::new(false)),
            machines: SMutex::new(BTreeMap::new()),
            wal_read_floors: SMutex::new(BTreeMap::new()),
            tx_machine,
            coordinators: SMutex::new(BTreeMap::new()),
            scheduler: SMutex::new(None),
            tasks: SMutex::new(Vec::new()),
            convergence_notify: mudu_sys::tokio::sync::Notify::new(),
            stopping,
            shared,
        }))
    }

    /// Phase B: installs one worker's data machine; when every worker is in,
    /// the apply driver and the convergence watcher start. Returns the
    /// machine (the worker's commit driver binds it).
    pub async fn install_worker(
        self: &Arc<Self>,
        worker_index: u32,
        worker: WorkerRuntime,
    ) -> RS<Arc<DataStreamMachine>> {
        worker
            .worker_x_contract()
            .set_tx_decision_client(self.decision_client(worker_index))?;
        let machine = Arc::new(
            DataStreamMachine::open(
                worker,
                self.raft.clone(),
                &self.apply_dir,
                &format!("data-{worker_index}.frontier"),
                data_stream_id(worker_index),
                self.snapshot_threshold_bytes,
            )
            .await?,
        );
        let mut machines = self
            .machines
            .lock()
            .map_err(|_| mudu_error!(ErrorCode::Internal, "data machines lock poisoned"))?;
        if machines.insert(worker_index, machine.clone()).is_some() {
            return Err(mudu_error!(
                ErrorCode::InvalidState,
                format!("worker {worker_index} data machine installed twice")
            ));
        }
        let complete = machines.len() as u32 == self.worker_count;
        drop(machines);
        if complete {
            self.start_drivers().await?;
        }
        Ok(machine)
    }

    /// Builds the scheduler over every installed machine and starts the
    /// apply driver and the convergence watcher (called once, when the last
    /// worker registers).
    async fn start_drivers(self: &Arc<Self>) -> RS<()> {
        let mut adapters: Vec<Arc<dyn StreamAdapter>> = Vec::new();
        {
            let machines = self
                .machines
                .lock()
                .map_err(|_| mudu_error!(ErrorCode::Internal, "data machines lock poisoned"))?;
            for (index, machine) in machines.iter() {
                adapters.push(Arc::new(LoggedStreamAdapter::new(
                    self.logs[&data_stream_id(*index)].clone(),
                    machine.clone(),
                )));
            }
        }
        adapters.push(Arc::new(LoggedStreamAdapter::new(
            self.logs[&TX_DECISION_STREAM].clone(),
            self.tx_machine.clone(),
        )));
        let state = self.registry.refresh().await?;
        let scheduler = if self.shared {
            Arc::new(StreamApplyScheduler::new_partial(&state, adapters)?)
        } else {
            Arc::new(StreamApplyScheduler::new(&state, adapters)?)
        };
        *self
            .scheduler
            .lock()
            .map_err(|_| mudu_error!(ErrorCode::Internal, "data scheduler lock poisoned"))? =
            Some(scheduler.clone());
        let mut tasks = self
            .tasks
            .lock()
            .map_err(|_| mudu_error!(ErrorCode::Internal, "data tasks lock poisoned"))?;
        let service = self.clone();
        let apply_stop = self.stopping.subscribe();
        tasks.push(background::spawn("data-apply", async move {
            run_apply_driver(service, scheduler, apply_stop).await
        })?);
        let service = self.clone();
        let convergence_stop = self.stopping.subscribe();
        tasks.push(background::spawn("data-convergence", async move {
            run_convergence(service, convergence_stop).await
        })?);
        Ok(())
    }

    /// The worker's committed WAL resume point (the driver's chain anchor).
    pub fn wal_resume(&self, worker_index: u32) -> RS<u64> {
        self.machines
            .lock()
            .map_err(|_| mudu_error!(ErrorCode::Internal, "data machines lock poisoned"))?
            .get(&worker_index)
            .ok_or_else(|| {
                mudu_error!(
                    ErrorCode::InvalidState,
                    format!("worker {worker_index} data machine not installed")
                )
            })?
            .wal_resume()
    }

    /// Pins the worker's WAL read floor so prefix GC cannot reclaim below
    /// `lsn` while the commit driver reads its next segment.
    pub fn pin_wal_read(&self, worker_index: u32, lsn: u64) {
        if let Ok(mut floors) = self.wal_read_floors.lock() {
            floors.insert(worker_index, lsn);
        }
    }

    /// Clears the worker's WAL read floor.
    pub fn unpin_wal_read(&self, worker_index: u32) {
        if let Ok(mut floors) = self.wal_read_floors.lock() {
            floors.remove(&worker_index);
        }
    }

    /// The worker's in-flight WAL read floor (`0` when no reader is pinned).
    pub fn wal_read_floor(&self, worker_index: u32) -> u64 {
        self.wal_read_floors
            .lock()
            .ok()
            .and_then(|floors| floors.get(&worker_index).copied())
            .unwrap_or(0)
    }

    /// Leader-side: appends one commit segment to the worker's data stream.
    /// Returns once the segment's progress is committed by the group.
    pub async fn append_data_segment(
        &self,
        worker_index: u32,
        first_lsn: u64,
        segment: DataSegment,
    ) -> RS<StreamCommitReceipt> {
        let payload = segment.encode()?;
        self.group
            .append(
                data_stream_id(worker_index),
                // The segment's first WAL LSN is the idempotency key: a
                // retried cut of the same segment dedups; a truncated
                // speculative segment frees the key for its replacement.
                u128::from(first_lsn),
                payload,
                Vec::new(),
            )
            .await
    }

    pub(super) async fn snapshot_after_local_apply(&self, worker_index: u32) -> RS<()> {
        let machine = self
            .machines
            .lock()?
            .get(&worker_index)
            .cloned()
            .ok_or_else(|| {
                mudu_error!(ErrorCode::InvalidState, "snapshot worker machine missing")
            })?;
        machine.snapshot_after_local_apply().await
    }

    /// Whether this node is the data-group leader AND converged (the write
    /// gate). The shared Raft view reads the same gate.
    pub fn is_serving_leader(&self) -> bool {
        self.raft.status().role == RaftRole::Leader && self.serving.load(Ordering::Relaxed)
    }

    /// The node's shared raft state.
    pub fn share(&self) -> &Arc<crate::raft_service::RaftShare> {
        &self.share
    }

    /// The shared raft handle of the data group.
    pub fn raft(&self) -> &Arc<RaftHandle> {
        &self.raft
    }

    /// One worker's committed view `(wal_resume, digest)` for status
    /// endpoints (synchronous; the machine's resume point plus the shared
    /// committed vector).
    pub fn committed_view(&self, worker_index: u32) -> Option<(u64, [u8; 32])> {
        let resume = self.wal_resume(worker_index).ok()?;
        let hash = self
            .share
            .committed_vector()
            .get(&worker_index)
            .map(|(_, hash)| *hash)
            .unwrap_or(crate::seg_hash::ZERO_SEG_HASH);
        Some((resume, hash))
    }

    /// One stream's committed progress, when any.
    pub async fn committed_progress(
        &self,
        stream_id: StreamId,
    ) -> RS<Option<mudu_raft::stream::StreamProgress>> {
        Ok(self
            .registry
            .refresh()
            .await?
            .committed()
            .get(&stream_id)
            .cloned())
    }

    /// The replication group (stream receive/fetch handlers route here).
    pub fn group(&self) -> &Arc<GenericReplicationGroup> {
        &self.group
    }

    /// Asks the convergence watcher to re-converge (a driver hit a chain
    /// break).
    pub fn request_convergence(&self) {
        self.serving.store(false, Ordering::Relaxed);
        self.convergence_notify.notify_waiters();
    }

    /// Stops the service: closes the write gate, stops consensus, joins the
    /// background tasks, then drains the logs. Consensus stops BEFORE the
    /// task joins: a task parked in a leader barrier or proposal wait is
    /// released by the driver shutdown (its wait errors out), so the join
    /// below can never be held by an in-flight consensus operation. The
    /// control plane uses the same ordering.
    pub fn stop(&self) -> RS<()> {
        if mudu_sys::task::async_::has_tokio_runtime() {
            return Err(mudu_error!(
                ErrorCode::InvalidState,
                "data plane stop must run outside the async executor"
            ));
        }
        self.set_serving(false);
        self.stopping.send_replace(true);
        let runtime = mudu_sys::task::async_::build_current_thread_runtime()?;
        let group = self.group.clone();
        let mut result = runtime.block_on(async move { group.shutdown().await });
        let tasks = std::mem::take(&mut *self.tasks.lock()?);
        for task in tasks {
            if let Err(error) = task.join() {
                if result.is_ok() {
                    result = Err(error);
                }
            }
        }
        result
    }

    /// The stream registry.
    pub(crate) fn registry(&self) -> &Arc<ReplicatedStreamRegistry> {
        &self.registry
    }

    /// Opens or closes the write gate (and the shared raft view feeding the
    /// workers' commit path).
    pub(crate) fn set_serving(&self, serving: bool) {
        self.serving.store(serving, Ordering::Relaxed);
        self.share.set_serving(serving);
    }

    /// Wakes the convergence watcher.
    pub(crate) async fn convergence_notified(&self) {
        self.convergence_notify.notified().await
    }

    /// One convergence pass: a current-term leader barrier, then catch-up of
    /// every locally missing committed prefix (which also truncates
    /// speculative tails).
    pub(crate) async fn converge_once(&self) -> RS<()> {
        self.registry.leader_view().await?;
        self.group.catch_up_missing().await?;
        Ok(())
    }

    /// Whether every worker's machine is installed (drivers running).
    pub fn is_complete(&self) -> bool {
        self.machines
            .lock()
            .map(|machines| machines.len() as u32 == self.worker_count)
            .unwrap_or(false)
    }
}

/// The data group's adapter capabilities (one kind per stream kind).
pub(crate) fn data_adapter_capabilities(_worker_count: u32) -> AdapterCapabilities {
    let mut capabilities = AdapterCapabilities::default();
    capabilities.add(DATA_STREAM_KIND, 1, 1);
    capabilities.add(TX_DECISION_KIND, 3, 1);
    capabilities
}

/// The per-worker data stream descriptor.
pub(crate) fn data_stream_descriptor(stream_id: StreamId) -> mudu_raft::stream::StreamDescriptor {
    mudu_raft::stream::StreamDescriptor {
        stream_id,
        stream_generation: 1,
        kind: DATA_STREAM_KIND.to_string(),
        codec_version: 1,
        digest_version: 1,
        budget: mudu_raft::stream::StreamBudget {
            max_record_bytes: DATA_SEGMENT_MAX_RECORD_BYTES,
            max_inflight_bytes: DATA_STREAM_INFLIGHT_BYTES,
            priority: 1,
        },
        requires: Vec::new(),
    }
}

/// The tx_decision stream descriptor.
pub(crate) fn tx_decision_descriptor() -> mudu_raft::stream::StreamDescriptor {
    mudu_raft::stream::StreamDescriptor {
        stream_id: TX_DECISION_STREAM,
        stream_generation: 1,
        kind: TX_DECISION_KIND.to_string(),
        codec_version: 3,
        digest_version: 1,
        budget: mudu_raft::stream::StreamBudget {
            max_record_bytes: TX_DECISION_MAX_RECORD_BYTES,
            max_inflight_bytes: 16 * 1024 * 1024,
            // Decisions gate in-doubt prepare recovery; never starve them.
            priority: 2,
        },
        requires: Vec::new(),
    }
}

/// Registers the tx_decision stream plus one data stream per worker.
/// `bootstrap_stop` aborts the wait promptly on server shutdown.
async fn ensure_data_streams_registered(
    registry: &Arc<ReplicatedStreamRegistry>,
    worker_count: u32,
    mut bootstrap_stop: Option<mudu_sys::sync::async_::stop_flag::StopRx>,
) -> RS<()> {
    let wait = async {
        loop {
            let state = registry.refresh().await?;
            let mut missing = !state.streams().contains_key(&TX_DECISION_STREAM);
            for worker_index in 0..worker_count {
                missing = missing || !state.streams().contains_key(&data_stream_id(worker_index));
            }
            if !missing {
                return Ok(());
            }
            if registry.leader_view().await.is_ok() {
                if !state.streams().contains_key(&TX_DECISION_STREAM) {
                    let _ = registry
                        .change(1, RegistryOp::Register(tx_decision_descriptor()))
                        .await;
                }
                for worker_index in 0..worker_count {
                    let stream_id = data_stream_id(worker_index);
                    if !state.streams().contains_key(&stream_id) {
                        let _ = registry
                            .change(
                                2 + u128::from(worker_index),
                                RegistryOp::Register(data_stream_descriptor(stream_id)),
                            )
                            .await;
                    }
                }
            }
            if let Some(stop) = &mut bootstrap_stop {
                mudu_sys::tokio::select! {
                    _ = mudu_sys::task::async_::sleep(Duration::from_millis(50)) => {}
                    _ = stop.changed() => {
                        return Err(mudu_error!(
                            ErrorCode::Interrupted,
                            "data stream registration aborted by shutdown"
                        ));
                    }
                }
            } else {
                mudu_sys::task::async_::sleep(Duration::from_millis(50)).await?;
            }
        }
    };
    mudu_sys::timeout(Duration::from_secs(60), wait)
        .await
        .ok_or_else(|| {
            mudu_error!(
                ErrorCode::TimedOut,
                "data stream registration did not complete in time"
            )
        })?
}

const APPLY_SWEEP: Duration = Duration::from_millis(50);
/// How often the convergence watcher re-evaluates a not-yet-serving leader.
const CONVERGENCE_RETRY: Duration = Duration::from_millis(200);

/// The apply driver: on a periodic sweep, fetches any missing committed
/// payload, then applies every newly committed stream record until no stream
/// progresses. It deliberately does NOT subscribe to per-commit
/// notifications: the commit watch channel treats a lagging subscriber as
/// fatal to the Raft driver, and apply rounds do unbounded storage I/O, so a
/// loaded group could otherwise lose its Raft driver to backpressure.
pub(super) async fn run_apply_driver(
    service: Arc<DataPlaneService>,
    scheduler: Arc<StreamApplyScheduler>,
    mut stop: mudu_sys::tokio::sync::watch::Receiver<bool>,
) -> RS<()> {
    let mut sweep = mudu_sys::tokio::time::interval(APPLY_SWEEP);
    sweep.set_missed_tick_behavior(mudu_sys::tokio::time::MissedTickBehavior::Skip);
    loop {
        mudu_sys::tokio::select! {
            _ = sweep.tick() => {}
            _ = stop.changed() => return Ok(()),
        }
        // The sink-pause test hook simulates a lagging replica by holding
        // this node's apply back.
        if service.share().sink_paused() {
            continue;
        }
        loop {
            match service.group().catch_up_missing().await {
                Ok(_) => {}
                Err(error) if *stop.borrow() => {
                    trace!("data apply driver exits during shutdown: {}", error);
                    return Ok(());
                }
                Err(error) => {
                    warn!("data stream catch-up failed: {}", error);
                    break;
                }
            }
            let state = match service.registry().refresh().await {
                Ok(state) => state,
                Err(error) if *stop.borrow() => {
                    trace!("data apply driver exits during shutdown: {}", error);
                    return Ok(());
                }
                Err(error) => return Err(error),
            };
            // Feed the shared observability view: per-worker committed WAL
            // points from the committed stream vector.
            for (stream_id, progress) in state.committed() {
                if let Some(worker_index) = super::worker_index_of_stream(*stream_id) {
                    service.share().record_committed(
                        worker_index,
                        progress.end_lsn,
                        progress.segment_digest,
                    );
                }
            }
            // A mid-shutdown apply failure is benign: the next boot's
            // convergence re-applies whatever the shutdown interrupted.
            let outcomes = match scheduler.apply_round(&state).await {
                Ok(outcomes) => outcomes,
                Err(error) if *stop.borrow() => {
                    trace!("data apply driver exits during shutdown: {}", error);
                    return Ok(());
                }
                Err(error) => return Err(error),
            };
            let mut progressed = false;
            for (stream, outcome) in outcomes {
                match outcome {
                    Ok(ApplyOutcome::Applied(_)) => progressed = true,
                    Ok(_) => {}
                    Err(error) => {
                        warn!(?stream, "data stream apply failed: {}", error);
                    }
                }
            }
            if !progressed {
                break;
            }
        }
    }
}

/// The convergence watcher: after this node becomes the data-group leader,
/// converge (leader barrier + catch-up + speculative-tail truncation) before
/// opening the write gate; losing leadership closes the gate immediately.
pub(super) async fn run_convergence(
    service: Arc<DataPlaneService>,
    mut stop: mudu_sys::tokio::sync::watch::Receiver<bool>,
) -> RS<()> {
    let mut status = service.raft().watch_status_async()?;
    let mut retry = mudu_sys::tokio::time::interval(CONVERGENCE_RETRY);
    retry.set_missed_tick_behavior(mudu_sys::tokio::time::MissedTickBehavior::Skip);
    // Whether the local gate is currently open (convergence succeeded since
    // the last leadership transition). Tracked locally so the barrier is not
    // re-proposed on every raft status update: each commit changes the status
    // (commit_index), and responding to that by re-converging would flood the
    // log with leader barriers in a self-reinforcing loop.
    let mut serving = false;
    loop {
        let mut converge = false;
        mudu_sys::tokio::select! {
            changed = status.changed() => {
                if changed.is_err() { return Ok(()); }
                if status.borrow_and_update().role == RaftRole::Leader {
                    // Only a fresh leadership transition re-opens convergence.
                    converge = !serving;
                } else {
                    serving = false;
                    service.set_serving(false);
                }
            }
            _ = retry.tick() => {
                converge = status.borrow().role == RaftRole::Leader && !serving;
            }
            _ = stop.changed() => return Ok(()),
            _ = service.convergence_notified() => {
                // An explicit wakeup (a commit driver failure) re-converges
                // even while serving, to re-establish the resume point.
                converge = status.borrow().role == RaftRole::Leader;
            }
        }
        if !converge {
            continue;
        }
        match service.converge_once().await {
            Ok(()) => {
                service.set_serving(true);
                serving = true;
            }
            Err(error) => {
                trace!("data group convergence not complete: {}", error);
                serving = false;
            }
        }
    }
}
