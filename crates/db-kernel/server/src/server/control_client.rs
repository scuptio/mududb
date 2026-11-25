//! The data-plane control client (phase 2 of the dedicated control plane):
//! a data node stops hosting a control Raft learner and instead subscribes to
//! the control group's committed catalog/epoch state through the cluster
//! channel.
//!
//! The client never votes and never runs a control Raft driver. It discovers
//! the current control leader, reads the committed prefix of the meta and
//! epoch streams (the control streams retain their full committed history),
//! applies them to the local catalog and the shared epoch oracle, and reports
//! its drained epoch back to the leader. Every read is continuity-checked
//! against the client's last applied prefix (identity, generation and digest),
//! so a restart, leader switch or disconnect can never apply a stale version
//! or skip an update; on divergence the client resets and re-syncs from
//! scratch.
//!
//! The client also serves the node's DDL face: it forwards catalog proposals
//! to the control leader and resolves them once the local catalog has applied
//! the committed record (DDL success is tied to local apply progress).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use ::catalog::contract::channel_kinds::CLUSTER_EPOCH_TELL;
use ::catalog::contract::control_plane::{
    ControlPlaneStateRequest, ControlStructsRequest, ControlStructsResponse, CONTROL_STREAM_EPOCH,
    CONTROL_STREAM_META,
};
use ::catalog::contract::derive_oid::derive_node_endpoint;
use ::catalog::contract::epoch_op::{EpochHeartbeat, EpochOp};
use ::catalog::contract::epoch_oracle::ClusterEpochState;
use ::catalog::contract::meta_op::{
    MetaGroupForward, MetaGroupRpcRequest, MetaGroupRpcResponse, MetaOp,
};
use ::mudu_raft::stream::{GroupIdobject, StreamId, StreamProgress};
use mudu::channel::{DeliveryMode, Envelope};
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_sys::sync::async_::stop_flag::{stop_channel, StopRx, StopTx};
use mudu_sys::sync::SMutex;
use mudu_sys::task::sync::SJoinHandle;
use mudu_utils::channel::{spawn_endpoint, Channel, EndpointGuard, ServiceHandle, ServiceRouter};
use repl::control_plane::{EpochCheckpoint, MetaApplier};
use repl::log_stream::FrontierStore;

use super::cluster_directory::ClusterDirectory;
use super::cluster_transport::ClusterTransport;
use crate::local_endpoint::{LocalEndpoint, WorkerRoute};

/// Poll interval of the subscribe loop (leader discovery, stream sync and the
/// drained-epoch heartbeat).
const CLIENT_POLL_INTERVAL: Duration = Duration::from_millis(50);
/// Deadline of one state-probe or committed-records read.
const CLIENT_RPC_TIMEOUT: Duration = Duration::from_secs(5);
/// Budget of one meta propose (commit + local apply).
const META_APPLY_TIMEOUT: Duration = Duration::from_secs(15);

/// State shared between the subscribe loop and the node's DDL face: the
/// current control leader and the set of locally-applied meta request ids.
struct ClientShared {
    /// `(leader, term)` of the control group as last discovered.
    leader: SMutex<(Option<u64>, u64)>,
    /// Request id → LSN of every locally-applied meta record.
    applied_requests: SMutex<HashMap<u128, u64>>,
    /// Fires whenever a new meta record is applied (DDL waiters re-check).
    meta_applied_notify: mudu_sys::tokio::sync::Notify,
}

impl ClientShared {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            leader: SMutex::new((None, 0)),
            applied_requests: SMutex::new(HashMap::new()),
            meta_applied_notify: mudu_sys::tokio::sync::Notify::new(),
        })
    }
}

/// The data-plane control client of one node.
pub struct DataPlaneControlClient {
    stop_tx: StopTx,
    join: Option<SJoinHandle<RS<()>>>,
    _endpoint_guard: EndpointGuard,
    _service_handle: ServiceHandle,
}

impl DataPlaneControlClient {
    /// Starts the client: installs the node endpoint, registers the DDL face
    /// and spawns the subscribe loop on a dedicated thread.
    #[allow(clippy::too_many_arguments)]
    pub fn start(
        server_instance_id: OID,
        directory: Arc<ClusterDirectory>,
        transport: Arc<ClusterTransport>,
        data_dir: &str,
        log_dir: &std::path::Path,
        route: WorkerRoute,
        epoch_state: Arc<ClusterEpochState>,
        oracle: Arc<::xcontract::epoch_oracle::ClusterEpochOracle>,
        control_gate: Arc<AtomicBool>,
    ) -> RS<Self> {
        let node_id = directory.self_node_id();
        let identity = super::cluster_directory::control_group_identity(directory.topology());
        let control_nodes = directory.topology().control_voters();
        let endpoint = LocalEndpoint::new(derive_node_endpoint(node_id), server_instance_id, route);
        let (channel, guard) = spawn_endpoint(
            server_instance_id,
            derive_node_endpoint(node_id),
            &endpoint.channel_ref(),
        )?;
        let shared = ClientShared::new();
        let service_handle = register_meta_face(&channel, shared.clone())?;
        let applier = Arc::new(super::meta_apply::NodeMetaApplier::new(
            data_dir,
            directory.clone(),
        ));
        let client_log_dir = log_dir.join("control_client");
        let (stop_tx, stop_rx) = stop_channel();
        let handle =
            mudu_sys::task::sync::spawn_thread_named("control-client".to_string(), move || {
                // The subscribe loop applies catalog meta ops, which open
                // relation mailboxes that spawn thread-confined tasks; those
                // need an active LocalSet, so run on a local task set (unlike
                // the bare current-thread runtime, which has none).
                let runtime = mudu_sys::task::async_::CurrentThreadTaskRuntime::new()?;
                runtime.block_on(run_client(
                    node_id,
                    identity,
                    control_nodes,
                    channel,
                    applier,
                    epoch_state,
                    control_gate,
                    oracle,
                    transport,
                    client_log_dir,
                    shared,
                    stop_rx,
                ))
            })?;
        Ok(Self {
            stop_tx,
            join: Some(handle),
            _endpoint_guard: guard,
            _service_handle: service_handle,
        })
    }

    /// Stops the subscribe loop and joins its thread.
    pub fn stop(mut self) -> RS<()> {
        self.stop_tx.stop();
        if let Some(handle) = self.join.take() {
            handle
                .join()
                .map_err(|_| mudu_error!(ErrorCode::Thread, "join control client error"))??;
        }
        Ok(())
    }
}

/// Registers the node's DDL face: forward a proposal to the control leader and
/// resolve it once the local catalog applies the committed record.
fn register_meta_face(channel: &Channel, shared: Arc<ClientShared>) -> RS<ServiceHandle> {
    let channel_for_closure = channel.clone();
    let router =
        ServiceRouter::new().handle::<MetaGroupRpcRequest, _, _>(move |request, _ctx| {
            let shared = shared.clone();
            let channel = channel_for_closure.clone();
            async move {
                match request {
                    MetaGroupRpcRequest::Propose { request_id, op } => {
                        let (leader, term) = *shared.leader.lock().map_err(|_| {
                            mudu_error!(ErrorCode::Internal, "leader slot poisoned")
                        })?;
                        let Some(leader) = leader else {
                            return Ok(MetaGroupRpcResponse::NotLeader { leader: None, term });
                        };
                        let forwarded = channel
                            .endpoint::<MetaGroupForward>(derive_node_endpoint(leader))
                            .tell(MetaGroupForward {
                                request_id,
                                op: *op,
                            })
                            .await;
                        if let Err(err) = forwarded {
                            return Ok(MetaGroupRpcResponse::Err(err.to_string()));
                        }
                        match wait_meta_applied(&shared, request_id).await {
                            Ok(()) => Ok(MetaGroupRpcResponse::Applied),
                            // The local apply did not land (a control leader
                            // switch, say): tell the caller to retry at the
                            // current leader instead of failing the DDL.
                            Err(_) => {
                                let (leader, term) = *shared.leader.lock().map_err(|_| {
                                    mudu_error!(ErrorCode::Internal, "leader slot poisoned")
                                })?;
                                Ok(MetaGroupRpcResponse::NotLeader { leader, term })
                            }
                        }
                    }
                }
            }
        })?;
    channel.serve(router)
}

/// Waits until the local catalog has applied the meta record with `request_id`.
async fn wait_meta_applied(shared: &ClientShared, request_id: u128) -> RS<()> {
    let wait = async {
        loop {
            let applied = shared
                .applied_requests
                .lock()
                .map_err(|_| mudu_error!(ErrorCode::Internal, "applied requests poisoned"))?
                .contains_key(&request_id);
            if applied {
                return Ok(());
            }
            shared.meta_applied_notify.notified().await;
        }
    };
    mudu_sys::timeout(META_APPLY_TIMEOUT, wait)
        .await
        .ok_or_else(|| {
            mudu_error!(
                ErrorCode::TimedOut,
                "control meta op apply timed out (commit or local apply)"
            )
        })?
}

/// The outcome of one stream sync pass.
enum Sync {
    /// Applied up to the committed frontier (possibly with no new records).
    Applied {
        /// The leader's committed frontier.
        committed: StreamProgress,
        /// The client's applied frontier after the pass.
        applied: StreamProgress,
    },
    /// The stream has no committed records yet: the client is trivially in
    /// sync.
    Empty,
    /// The client must reset and re-sync from scratch.
    Resync,
}

/// The durable applied frontiers of the control client, recovered on startup
/// and persisted as the meta/epoch streams converge.
struct ClientFrontiers {
    meta: FrontierStore,
    epoch: FrontierStore,
    meta_applied: Option<StreamProgress>,
    meta_persisted: Option<StreamProgress>,
    meta_committed: Option<StreamProgress>,
    epoch_applied: Option<StreamProgress>,
    epoch_persisted: Option<StreamProgress>,
    epoch_checkpoint: EpochCheckpoint,
}

impl ClientFrontiers {
    /// Opens the frontier stores and recovers the last applied prefixes and
    /// the epoch checkpoint.
    async fn open(log_dir: &std::path::Path) -> RS<Self> {
        let fs = mudu_sys::fs::tokio_fs();
        repl::durable_io::ensure_dir(fs.as_ref(), log_dir).await?;
        let (meta, meta_recovered) = FrontierStore::open(
            log_dir,
            "client-meta.frontier",
            StreamId(CONTROL_STREAM_META),
            1,
        )
        .await?;
        let (epoch, epoch_recovered) = FrontierStore::open(
            log_dir,
            "client-epoch.frontier",
            StreamId(CONTROL_STREAM_EPOCH),
            1,
        )
        .await?;
        let meta_applied = meta_recovered.map(|(progress, _)| progress);
        let mut epoch_applied = None;
        let mut epoch_checkpoint = EpochCheckpoint::initial();
        if let Some((progress, business)) = epoch_recovered {
            epoch_applied = Some(progress);
            if !business.is_empty() {
                epoch_checkpoint = rmp_serde::from_slice(&business).map_err(|e| {
                    mudu_error!(ErrorCode::Decode, "decode epoch client checkpoint", e)
                })?;
            }
        }
        Ok(Self {
            meta,
            epoch,
            meta_applied: meta_applied.clone(),
            meta_persisted: meta_applied,
            meta_committed: None,
            epoch_applied: epoch_applied.clone(),
            epoch_persisted: epoch_applied,
            epoch_checkpoint,
        })
    }

    /// Whether the local catalog has applied every committed meta record (or
    /// the meta stream is empty).
    fn synced(&self) -> bool {
        match (&self.meta_applied, &self.meta_committed) {
            (None, None) => true,
            (Some(applied), Some(committed)) => applied == committed,
            _ => false,
        }
    }

    /// Persists the frontiers that have advanced since the last persist.
    async fn persist(&mut self) -> RS<()> {
        if self.meta_applied != self.meta_persisted {
            if let Some(progress) = &self.meta_applied {
                self.meta.store(progress, &[]).await?;
            }
            self.meta_persisted = self.meta_applied.clone();
        }
        if self.epoch_applied != self.epoch_persisted {
            if let Some(progress) = &self.epoch_applied {
                let business = rmp_serde::to_vec(&self.epoch_checkpoint).map_err(|e| {
                    mudu_error!(ErrorCode::Encode, "encode epoch client checkpoint", e)
                })?;
                self.epoch.store(progress, &business).await?;
            }
            self.epoch_persisted = self.epoch_applied.clone();
        }
        Ok(())
    }
}

/// Syncs the meta and epoch streams against the committed prefixes and reports
/// the drained epoch, updating the frontier state in place.
#[allow(clippy::too_many_arguments)]
async fn sync_streams(
    node_id: u64,
    channel: &Channel,
    identity: &GroupIdobject,
    leader_id: u64,
    term: u64,
    applier: &dyn MetaApplier,
    shared: &ClientShared,
    oracle: &::xcontract::epoch_oracle::ClusterEpochOracle,
    transport: &ClusterTransport,
    epoch_state: &ClusterEpochState,
    frontiers: &mut ClientFrontiers,
) -> RS<()> {
    match sync_meta(
        channel,
        identity,
        leader_id,
        frontiers.meta_applied.as_ref(),
        applier,
        shared,
    )
    .await
    {
        Ok(Sync::Applied { committed, applied }) => {
            frontiers.meta_applied = Some(applied);
            frontiers.meta_committed = Some(committed);
        }
        Ok(Sync::Empty) | Ok(Sync::Resync) => {
            frontiers.meta_applied = None;
            frontiers.meta_committed = None;
        }
        // Transient (timeout, nack, leadership loss): retry next poll.
        Err(_) => {}
    }
    match sync_epoch(
        channel,
        identity,
        leader_id,
        frontiers.epoch_applied.as_ref(),
        &frontiers.epoch_checkpoint,
    )
    .await
    {
        Ok((Sync::Applied { applied, .. }, checkpoint)) => {
            frontiers.epoch_applied = Some(applied);
            frontiers.epoch_checkpoint = checkpoint;
            publish_epoch(epoch_state, &frontiers.epoch_checkpoint);
        }
        Ok((Sync::Empty, _)) => {
            frontiers.epoch_applied = None;
        }
        Ok((Sync::Resync, _)) => {
            frontiers.epoch_applied = None;
            frontiers.epoch_checkpoint = EpochCheckpoint::initial();
            publish_epoch(epoch_state, &frontiers.epoch_checkpoint);
        }
        Err(_) => {}
    }
    report_heartbeat(node_id, leader_id, term, oracle, transport)
}

/// The subscribe loop: discover the leader, sync the meta and epoch streams,
/// gate on the catalog sync and report the drained epoch.
#[allow(clippy::too_many_arguments)]
async fn run_client(
    node_id: u64,
    identity: GroupIdobject,
    control_nodes: Vec<u64>,
    channel: Channel,
    applier: Arc<dyn MetaApplier>,
    epoch_state: Arc<ClusterEpochState>,
    control_gate: Arc<AtomicBool>,
    oracle: Arc<::xcontract::epoch_oracle::ClusterEpochOracle>,
    transport: Arc<ClusterTransport>,
    log_dir: PathBuf,
    shared: Arc<ClientShared>,
    stop_rx: StopRx,
) -> RS<()> {
    control_gate.store(false, Ordering::Relaxed);
    let mut frontiers = ClientFrontiers::open(&log_dir).await?;
    publish_epoch(&epoch_state, &frontiers.epoch_checkpoint);

    loop {
        if stop_rx.is_stopped() {
            return Ok(());
        }
        // Leader discovery: update the shared slot on success; on a transient
        // probe failure keep the last known leader so the DDL face keeps
        // forwarding (a stale leader returns NotLeader and the caller retries).
        if let Some((leader_id, leader_term, epoch_watermarks)) =
            discover_leader(&channel, &control_nodes, &identity).await
        {
            *shared
                .leader
                .lock()
                .map_err(|_| mudu_error!(ErrorCode::Internal, "leader slot poisoned"))? =
                (Some(leader_id), leader_term);
            // Merge the leader's in-memory epoch watermarks (monotonic): under
            // the sparse epoch protocol the stable/current edges run ahead of
            // the committed records, bounded by the committed reserve.
            merge_epoch_checkpoint(&mut frontiers.epoch_checkpoint, &epoch_watermarks);
            publish_epoch(&epoch_state, &frontiers.epoch_checkpoint);
        }

        let (leader, term) = *shared
            .leader
            .lock()
            .map_err(|_| mudu_error!(ErrorCode::Internal, "leader slot poisoned"))?;

        if let Some(leader_id) = leader {
            sync_streams(
                node_id,
                &channel,
                &identity,
                leader_id,
                term,
                applier.as_ref(),
                &shared,
                &oracle,
                &transport,
                &epoch_state,
                &mut frontiers,
            )
            .await?;
        } else {
            control_gate.store(false, Ordering::Relaxed);
        }

        frontiers.persist().await?;
        // The control gate follows the catalog sync.
        control_gate.store(frontiers.synced(), Ordering::Relaxed);

        mudu_sys::task::async_::sleep(CLIENT_POLL_INTERVAL).await?;
    }
}

/// Probes the control nodes for the current leader, its term and its epoch
/// watermarks.
async fn discover_leader(
    channel: &Channel,
    control_nodes: &[u64],
    identity: &GroupIdobject,
) -> Option<(u64, u64, ::catalog::contract::control_plane::EpochStateWire)> {
    for node_id in control_nodes {
        let request = ControlPlaneStateRequest {
            cluster_id: identity.cluster_id,
            group_id: identity.group_id.0,
            group_generation: identity.generation,
        };
        let Ok(response) = channel
            .endpoint::<ControlPlaneStateRequest>(derive_node_endpoint(*node_id))
            .ask(request, CLIENT_RPC_TIMEOUT)
            .await
        else {
            continue;
        };
        if let Some(leader_id) = response.leader_id {
            return Some((leader_id, response.term, response.epoch));
        }
    }
    None
}

/// Reads the committed meta records from `applied + 1` and applies them to the
/// local catalog, recording each applied request id for the DDL face.
async fn sync_meta(
    channel: &Channel,
    identity: &GroupIdobject,
    leader_id: u64,
    applied: Option<&StreamProgress>,
    applier: &dyn MetaApplier,
    shared: &ClientShared,
) -> RS<Sync> {
    let response =
        fetch_records(channel, identity, leader_id, CONTROL_STREAM_META, applied).await?;
    if response.resync {
        return Ok(Sync::Resync);
    }
    let Some(committed) = response.committed else {
        return Ok(Sync::Empty);
    };
    let committed = wire_to_progress(committed);
    let mut applied_after = applied.cloned();
    let mut new_requests = Vec::new();
    for record in response.records {
        let op: MetaOp = rmp_serde::from_slice(&record.payload)
            .map_err(|e| mudu_error!(ErrorCode::Decode, "decode meta control record", e))?;
        let progress = wire_to_progress(record.progress);
        applier.apply(op, progress.end_lsn).await?;
        applied_after = Some(progress);
        new_requests.push((record.request_id, record.lsn));
    }
    if !new_requests.is_empty() {
        let mut applied_requests = shared
            .applied_requests
            .lock()
            .map_err(|_| mudu_error!(ErrorCode::Internal, "applied requests poisoned"))?;
        for (request_id, lsn) in new_requests {
            applied_requests.insert(request_id, lsn);
        }
        drop(applied_requests);
        shared.meta_applied_notify.notify_waiters();
    }
    let Some(applied_after) = applied_after else {
        return Ok(Sync::Resync);
    };
    Ok(Sync::Applied {
        committed,
        applied: applied_after,
    })
}

/// Reads the committed epoch records from `applied + 1` and advances the
/// epoch checkpoint (published to the shared oracle by the caller).
async fn sync_epoch(
    channel: &Channel,
    identity: &GroupIdobject,
    leader_id: u64,
    applied: Option<&StreamProgress>,
    checkpoint: &EpochCheckpoint,
) -> RS<(Sync, EpochCheckpoint)> {
    let response =
        fetch_records(channel, identity, leader_id, CONTROL_STREAM_EPOCH, applied).await?;
    if response.resync {
        return Ok((Sync::Resync, checkpoint.clone()));
    }
    let Some(committed) = response.committed else {
        return Ok((Sync::Empty, checkpoint.clone()));
    };
    let committed = wire_to_progress(committed);
    let mut checkpoint = checkpoint.clone();
    let mut applied_after = applied.cloned();
    for record in response.records {
        let op: EpochOp = rmp_serde::from_slice(&record.payload)
            .map_err(|e| mudu_error!(ErrorCode::Decode, "decode epoch control record", e))?;
        checkpoint = checkpoint.advance(op)?;
        applied_after = Some(wire_to_progress(record.progress));
    }
    let Some(applied_after) = applied_after else {
        return Ok((Sync::Resync, checkpoint));
    };
    Ok((
        Sync::Applied {
            committed,
            applied: applied_after,
        },
        checkpoint,
    ))
}

/// Reads one stream's committed records from the leader.
async fn fetch_records(
    channel: &Channel,
    identity: &GroupIdobject,
    leader_id: u64,
    stream_id: u64,
    applied: Option<&StreamProgress>,
) -> RS<ControlStructsResponse> {
    let from_lsn = applied.map_or(0, |progress| progress.end_lsn.saturating_add(1));
    let request = ControlStructsRequest {
        cluster_id: identity.cluster_id,
        group_id: identity.group_id.0,
        group_generation: identity.generation,
        stream_id,
        stream_generation: 1,
        from_lsn,
        applied: applied.map(progress_to_wire),
    };
    channel
        .endpoint::<ControlStructsRequest>(derive_node_endpoint(leader_id))
        .ask(request, CLIENT_RPC_TIMEOUT)
        .await
}

/// Reports this node's drained epoch to the control leader.
fn report_heartbeat(
    node_id: u64,
    leader_id: u64,
    term: u64,
    oracle: &::xcontract::epoch_oracle::ClusterEpochOracle,
    transport: &ClusterTransport,
) -> RS<()> {
    let (closed_epoch, drained_epoch) = oracle.closed_drained_epochs()?;
    let heartbeat = EpochHeartbeat {
        node_id,
        control_term: term,
        closed_epoch,
        drained_epoch,
    };
    let payload = rmp_serde::to_vec(&heartbeat)
        .map_err(|e| mudu_error!(ErrorCode::Encode, "encode epoch heartbeat", e))?;
    let envelope = Envelope::new(
        0,
        None,
        derive_node_endpoint(node_id),
        derive_node_endpoint(leader_id),
        CLUSTER_EPOCH_TELL,
        payload,
        DeliveryMode::FireAndForget,
    );
    transport.send_envelope(envelope)
}

/// Publishes an epoch checkpoint to the shared cluster epoch state.
fn publish_epoch(state: &ClusterEpochState, checkpoint: &EpochCheckpoint) {
    state.stable.fetch_max(checkpoint.stable, Ordering::AcqRel);
    state
        .current
        .fetch_max(checkpoint.current, Ordering::AcqRel);
    state.stable_notify.notify_waiters();
}

/// Merges the leader's in-memory epoch watermarks into the local checkpoint,
/// monotonically (never regressing the stable/current/reserved edges). The
/// leader's stable/current may run ahead of the committed records under the
/// sparse epoch protocol; the committed reserve is the authoritative bound.
fn merge_epoch_checkpoint(
    checkpoint: &mut EpochCheckpoint,
    state: &::catalog::contract::control_plane::EpochStateWire,
) {
    checkpoint.stable = checkpoint.stable.max(state.stable);
    checkpoint.current = checkpoint.current.max(state.current);
    checkpoint.reserved_upper = checkpoint.reserved_upper.max(state.reserved_upper);
}

/// Converts a replication `StreamProgress` to its wire mirror.
fn progress_to_wire(
    progress: &StreamProgress,
) -> ::catalog::contract::control_plane::StreamProgressWire {
    ::catalog::contract::control_plane::StreamProgressWire {
        stream_id: progress.stream_id.0,
        stream_generation: progress.stream_generation,
        end_lsn: progress.end_lsn,
        segment_digest: progress.segment_digest,
    }
}

/// Converts a wire `StreamProgressWire` back to a replication `StreamProgress`.
fn wire_to_progress(
    wire: ::catalog::contract::control_plane::StreamProgressWire,
) -> StreamProgress {
    StreamProgress {
        stream_id: StreamId(wire.stream_id),
        stream_generation: wire.stream_generation,
        end_lsn: wire.end_lsn,
        segment_digest: wire.segment_digest,
    }
}
