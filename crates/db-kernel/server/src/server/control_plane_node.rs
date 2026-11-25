//! The server-side wiring of the unified control plane: one merged control Raft group
//! per cluster whose log carries only stream progress references, while
//! catalog and epoch records replicate through the control plane's two
//! external log streams.
//!
//! This module owns the cluster-channel plumbing the layering rules keep out
//! of `repl`: the node endpoint's registration, the raft send path over the
//! cluster transport, the stream push/fetch handlers, the client RPC face
//! (unchanged `MetaGroupRpcRequest` contract), and the epoch heartbeat
//! reporter.

use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Duration;

use ::catalog::contract::channel_kinds::{
    CLUSTER_CONTROL_RAFT, CLUSTER_EPOCH_TELL, CLUSTER_META_FORWARD,
};
use ::catalog::contract::control_plane::{
    ControlPlaneStateRequest, ControlPlaneStateResponse, ControlStructWire, ControlStructsRequest,
    ControlStructsResponse, StreamFetchRequest, StreamFetchResponse, StreamProgressRequest,
    StreamProgressResponse, StreamProgressWire, StreamPushRequest, StreamPushResponse,
};
use ::catalog::contract::derive_oid::derive_node_endpoint;
use ::catalog::contract::epoch_op::EpochHeartbeat;
use ::catalog::contract::epoch_oracle::ClusterEpochState;
use ::catalog::contract::meta_op::{
    MetaGroupForward, MetaGroupRpcRequest, MetaGroupRpcResponse, MetaOp,
};
use ::mudu_raft::stream::{GroupId, GroupIdobject, StreamId, StreamProgress};
use ::mudu_raft::{RaftCfg, RaftHandle, RaftMsg, RaftTransport};
use mudu::channel::{DeliveryMode, Envelope};
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_sys::sync::async_::stop_flag::{stop_channel, StopTx};
use mudu_sys::task::sync::SJoinHandle;
use mudu_utils::channel::{spawn_endpoint, Channel, ServiceRouter};
use repl::control_plane::{ControlPlaneCfg, ControlPlaneService};
use repl::log_stream::StreamFetch;

use super::cluster_directory::ClusterDirectory;
use super::cluster_transport::ClusterTransport;
use super::control_plane_transport::{
    ack_to_wire, round_from_wire, ClusterStreamTransport, ReceiverSessions,
};
use super::meta_apply::NodeMetaApplier;
use crate::local_endpoint::{LocalEndpoint, WorkerRoute};

/// How often a node reports its drained epoch to the control-plane leader.
const EPOCH_HEARTBEAT_INTERVAL: Duration = Duration::from_millis(20);
/// Leader-side answer budget of one meta proposal (commit + local apply).
const META_APPLY_TIMEOUT: Duration = Duration::from_secs(15);

/// Raft transport of the control group over the cluster transport.
struct ControlRaftTransport {
    node_id: u64,
    transport: Arc<ClusterTransport>,
}

impl RaftTransport for ControlRaftTransport {
    fn send(&self, to: u64, msg: RaftMsg) -> RS<()> {
        let payload = rmp_serde::to_vec(&msg)
            .map_err(|e| mudu_error!(ErrorCode::Encode, "encode control raft msg error", e))?;
        let envelope = Envelope::new(
            0,
            None,
            derive_node_endpoint(self.node_id),
            derive_node_endpoint(to),
            CLUSTER_CONTROL_RAFT,
            payload,
            DeliveryMode::FireAndForget,
        );
        self.transport.send_envelope(envelope)
    }
}

/// The control-plane wiring of one node: the unified control group replica,
/// its channel endpoint, the stream transports and the epoch reporter.
pub struct ControlPlaneNode {
    service: Arc<ControlPlaneService>,
    channel: Channel,
    _endpoint_guard: mudu_utils::channel::EndpointGuard,
    _service_handle: mudu_utils::channel::ServiceHandle,
    reporter_stop: StopTx,
    reporter_join: Option<SJoinHandle<RS<()>>>,
}

impl ControlPlaneNode {
    /// Starts the control-plane replica of this node. In the Combined
    /// deployment `shared` carries the one replication group (raft, registry,
    /// streams over the replication channel); in scale-out `shared` is `None`
    /// and this node runs its own control raft group over the cluster channel.
    /// `bootstrap_stop` aborts the startup stream-registration wait promptly
    /// when the server is shutting down.
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
        shared: Option<Arc<::repl::log_stream::SharedGroup>>,
        bootstrap_stop: Option<mudu_sys::sync::async_::stop_flag::StopRx>,
        sparse_epoch: bool,
    ) -> RS<Self> {
        let node_id = directory.self_node_id();
        let topology = directory.topology();

        let endpoint = LocalEndpoint::new(derive_node_endpoint(node_id), server_instance_id, route);
        let (channel, guard) = spawn_endpoint(
            server_instance_id,
            derive_node_endpoint(node_id),
            &endpoint.channel_ref(),
        )?;

        let cfg = ControlPlaneCfg {
            node_id,
            voters: topology.control_voters(),
            learners: topology.control_learners(),
            epoch_participants: topology.data_nodes(),
            log_dir: log_dir.join("control_plane"),
            tick_ms: 25,
            election_timeout_ticks: 4,
            advance_interval: Duration::from_millis(500),
            identity: super::cluster_directory::control_group_identity(topology),
            epoch_state,
            control_gate: control_synced_gate(node_id),
            sparse_epoch,
            bootstrap_stop,
        };
        let applier = Arc::new(NodeMetaApplier::new(data_dir, directory.clone()));
        let runtime = mudu_sys::task::async_::build_current_thread_runtime()?;
        let (service, sessions) = match shared {
            Some(shared) => {
                // Combined: attach to the shared group; its raft frames and
                // stream push/fetch flow over the replication channel, so no
                // per-control raft handler or cluster-channel stream handlers.
                let service = runtime.block_on(ControlPlaneService::start_on_shared_group(
                    cfg, shared, applier,
                ))?;
                (service, None)
            }
            None => {
                // Scale-out: own control raft group over the cluster channel.
                let (service, sessions) = start_with_own_raft(
                    cfg,
                    node_id,
                    log_dir,
                    transport.clone(),
                    channel.clone(),
                    directory.clone(),
                    applier,
                    &runtime,
                )?;
                (service, Some(sessions))
            }
        };
        let service_handle =
            register_control_handlers(&channel, service.clone(), directory.clone(), sessions)?;
        let (reporter_stop, reporter_join) =
            spawn_epoch_reporter(node_id, service.clone(), transport, oracle)?;
        Ok(Self {
            service,
            channel,
            _endpoint_guard: guard,
            _service_handle: service_handle,
            reporter_stop,
            reporter_join: Some(reporter_join),
        })
    }

    /// The control-plane service.
    pub fn service(&self) -> &Arc<ControlPlaneService> {
        &self.service
    }

    /// The node endpoint's typed channel.
    pub fn channel(&self) -> &Channel {
        &self.channel
    }

    /// Stops the reporter and the control-plane replica.
    pub fn stop(mut self) -> RS<()> {
        self.reporter_stop.stop();
        if let Some(handle) = self.reporter_join.take() {
            handle
                .join()
                .map_err(|_| mudu_error!(ErrorCode::Thread, "join epoch reporter error"))??;
        }
        self.service.stop()
    }
}

/// The shared serving gate of this node's data plane: the control plane holds
/// the same flag and opens it once the local catalog is synced (A-05). Falls
/// back to a permanently-open gate when the data plane's raft share is not
/// registered (it always is in cluster mode).
pub(crate) fn control_synced_gate(node_id: u64) -> Arc<AtomicBool> {
    ::repl::raft_share_for_node(node_id)
        .map(|share| share.control_synced())
        .unwrap_or_else(|| Arc::new(AtomicBool::new(true)))
}

/// The scale-out start path: this node runs its own control raft group over
/// the cluster channel.
#[allow(clippy::too_many_arguments)]
fn start_with_own_raft(
    cfg: ControlPlaneCfg,
    node_id: u64,
    log_dir: &std::path::Path,
    transport: Arc<ClusterTransport>,
    channel: Channel,
    directory: Arc<ClusterDirectory>,
    applier: Arc<NodeMetaApplier>,
    runtime: &mudu_sys::task::async_::TaskRuntime,
) -> RS<(Arc<ControlPlaneService>, Arc<ReceiverSessions>)> {
    let raft_cfg = RaftCfg::new(
        node_id,
        cfg.voters.clone(),
        25,
        4,
        log_dir.join("control_plane").join("raft"),
    )
    // Per-node election-timeout phasing (see `raft_service`): node
    // ids are 64-bit name hashes, so reduce to a bounded tick phase.
    .with_election_timeout_offset(node_id % 4)
    .with_learners(cfg.learners.clone());
    let raft = Arc::new(RaftHandle::start(
        raft_cfg,
        Arc::new(ControlRaftTransport {
            node_id,
            transport: transport.clone(),
        }),
    )?);
    register_control_raft_handler(&channel, raft.clone(), directory.clone())?;
    let sessions = Arc::new(ReceiverSessions::new());
    let stream_transport = ClusterStreamTransport::new(node_id, channel.clone());
    // A failed assembly (e.g. the registration wait aborted by
    // shutdown) must stop the raft driver it leaves behind.
    let service = match runtime.block_on(ControlPlaneService::start_on_raft(
        cfg,
        raft.clone(),
        Arc::new(stream_transport),
        applier,
    )) {
        Ok(service) => service,
        Err(error) => {
            let _ = raft.stop();
            return Err(error);
        }
    };
    Ok((service, sessions))
}

/// Registers the control group's raft-frame handler (node endpoint ↔ node
/// endpoint, fire-and-forget).
fn register_control_raft_handler(
    channel: &Channel,
    raft: Arc<RaftHandle>,
    directory: Arc<ClusterDirectory>,
) -> RS<()> {
    channel.raw().register_kind_handler(
        CLUSTER_CONTROL_RAFT,
        Arc::new(move |envelope: Envelope| {
            let raft = raft.clone();
            let directory = directory.clone();
            Box::pin(async move {
                let from = directory.node_of_worker(*envelope.src()).ok_or_else(|| {
                    mudu_error!(
                        ErrorCode::ObjectNotFound,
                        format!(
                            "control raft frame from unknown endpoint {}",
                            envelope.src()
                        )
                    )
                })?;
                let msg: RaftMsg = rmp_serde::from_slice(envelope.payload()).map_err(|e| {
                    mudu_error!(ErrorCode::Decode, "decode control raft msg error", e)
                })?;
                raft.deliver(from, msg)
            })
        }),
    )
}

/// Registers every typed handler of the control plane: the meta client RPC
/// (propose / tx-status query) and, in scale-out, the cluster-channel stream
/// push/fetch/progress RPCs (the Combined deployment routes streams over the
/// replication channel instead).
fn register_control_handlers(
    channel: &Channel,
    service: Arc<ControlPlaneService>,
    directory: Arc<ClusterDirectory>,
    sessions: Option<Arc<ReceiverSessions>>,
) -> RS<mudu_utils::channel::ServiceHandle> {
    let router = ServiceRouter::new();
    let router = register_meta_rpc_handler(router, channel, service.clone())?;
    let router = register_control_state_handler(router, service.clone())?;
    let router = register_control_records_handler(router, service.clone())?;
    let router = match sessions {
        Some(sessions) => {
            let router =
                register_stream_push_handler(router, service.clone(), directory.clone(), sessions)?;
            let router = register_stream_fetch_handler(router, service.clone(), directory.clone())?;
            register_stream_progress_handler(router, service.clone(), directory)?
        }
        None => router,
    };
    let handle = channel.serve(router)?;
    register_forward_handler(channel, service.clone())?;
    register_epoch_heartbeat_handler(channel, service)?;
    Ok(handle)
}

/// The meta client RPC handler: propose (leader/forward) and the
/// metadata proposal.
fn register_meta_rpc_handler(
    router: ServiceRouter,
    channel: &Channel,
    service: Arc<ControlPlaneService>,
) -> RS<ServiceRouter> {
    let channel = channel.clone();
    router.handle::<MetaGroupRpcRequest, _, _>(move |request, _ctx| {
        let service = service.clone();
        let channel = channel.clone();
        async move {
            match request {
                MetaGroupRpcRequest::Propose { request_id, op } => {
                    Ok(drive_meta_proposal(service, channel, request_id, *op).await)
                }
            }
        }
    })
}

/// The control-plane state probe handler (data-plane control client → control
/// node): answers with the control leader, its term and the committed
/// frontiers, so a client converges without joining the control Raft group.
fn register_control_state_handler(
    router: ServiceRouter,
    service: Arc<ControlPlaneService>,
) -> RS<ServiceRouter> {
    router.handle::<ControlPlaneStateRequest, _, _>(move |request, _ctx| {
        let service = service.clone();
        async move {
            let identity = GroupIdobject {
                cluster_id: request.cluster_id,
                group_id: GroupId(request.group_id),
                generation: request.group_generation,
            };
            if !service.matches_identity(&identity).await? {
                return Ok(ControlPlaneStateResponse {
                    is_leader: false,
                    leader_id: None,
                    term: 0,
                    committed: Vec::new(),
                    epoch: ::catalog::contract::control_plane::EpochStateWire {
                        stable: 0,
                        current: 0,
                        reserved_upper: 0,
                    },
                });
            }
            let committed = service
                .committed_frontiers()
                .await?
                .into_iter()
                .map(progress_to_wire)
                .collect();
            let epoch = service.epoch_checkpoint()?;
            Ok(ControlPlaneStateResponse {
                is_leader: service.is_leader(),
                leader_id: service.leader(),
                term: service.term(),
                committed,
                epoch: ::catalog::contract::control_plane::EpochStateWire {
                    stable: epoch.stable,
                    current: epoch.current,
                    reserved_upper: epoch.reserved_upper,
                },
            })
        }
    })
}

/// The control-plane committed-records read handler (data-plane control client
/// → control leader): serves only the committed prefix of one control stream.
fn register_control_records_handler(
    router: ServiceRouter,
    service: Arc<ControlPlaneService>,
) -> RS<ServiceRouter> {
    router.handle::<ControlStructsRequest, _, _>(move |request, _ctx| {
        let service = service.clone();
        async move {
            let identity = GroupIdobject {
                cluster_id: request.cluster_id,
                group_id: GroupId(request.group_id),
                generation: request.group_generation,
            };
            let applied = request.applied.map(|wire| StreamProgress {
                stream_id: StreamId(wire.stream_id),
                stream_generation: wire.stream_generation,
                end_lsn: wire.end_lsn,
                segment_digest: wire.segment_digest,
            });
            let outcome = service
                .committed_records(
                    &identity,
                    StreamId(request.stream_id),
                    request.from_lsn,
                    applied.as_ref(),
                )
                .await?;
            match outcome {
                repl::control_plane::CommittedRead::Structs { committed, records } => {
                    Ok(ControlStructsResponse {
                        resync: false,
                        committed: Some(progress_to_wire(committed)),
                        records: records
                            .into_iter()
                            .map(|(lsn, request_id, progress, payload)| ControlStructWire {
                                lsn,
                                request_id,
                                progress: progress_to_wire(progress),
                                payload,
                            })
                            .collect(),
                    })
                }
                repl::control_plane::CommittedRead::Empty => Ok(ControlStructsResponse {
                    resync: false,
                    committed: None,
                    records: Vec::new(),
                }),
                repl::control_plane::CommittedRead::Resync => Ok(ControlStructsResponse {
                    resync: true,
                    committed: None,
                    records: Vec::new(),
                }),
            }
        }
    })
}

/// Converts a replication `StreamProgress` to its wire mirror.
fn progress_to_wire(progress: StreamProgress) -> StreamProgressWire {
    StreamProgressWire {
        stream_id: progress.stream_id.0,
        stream_generation: progress.stream_generation,
        end_lsn: progress.end_lsn,
        segment_digest: progress.segment_digest,
    }
}

/// The stream push handler: a member receives one leader frame.
fn register_stream_push_handler(
    router: ServiceRouter,
    service: Arc<ControlPlaneService>,
    directory: Arc<ClusterDirectory>,
    sessions: Arc<ReceiverSessions>,
) -> RS<ServiceRouter> {
    router.handle::<StreamPushRequest, _, _>(move |request, ctx| {
        let service = service.clone();
        let directory = directory.clone();
        let sessions = sessions.clone();
        async move {
            let from = directory.node_of_worker(ctx.src()).ok_or_else(|| {
                mudu_error!(
                    ErrorCode::ObjectNotFound,
                    format!("stream push from unknown endpoint {}", ctx.src())
                )
            })?;
            let session = sessions.session_for(from, request.session_id)?;
            let round = round_from_wire(&request.round);
            let ack = service
                .receive_stream_frame(from, &session, &round, &request.frame)
                .await?;
            Ok(StreamPushResponse {
                ack: ack.map(|ack| ack_to_wire(&ack)),
            })
        }
    })
}

/// The stream fetch handler: a member pulls one durable frame.
fn register_stream_fetch_handler(
    router: ServiceRouter,
    service: Arc<ControlPlaneService>,
    directory: Arc<ClusterDirectory>,
) -> RS<ServiceRouter> {
    router.handle::<StreamFetchRequest, _, _>(move |request, ctx| {
        let service = service.clone();
        let directory = directory.clone();
        async move {
            // Only group members may pull stream frames.
            let _from = directory.node_of_worker(ctx.src()).ok_or_else(|| {
                mudu_error!(
                    ErrorCode::ObjectNotFound,
                    format!("stream fetch from unknown endpoint {}", ctx.src())
                )
            })?;
            let frame = service
                .fetch_stream_frame(StreamFetch {
                    identity: mudu_raft::stream::GroupIdobject {
                        cluster_id: request.cluster_id,
                        group_id: GroupId(request.group_id),
                        generation: request.group_generation,
                    },
                    stream_id: mudu_raft::stream::StreamId(request.stream_id),
                    stream_generation: request.stream_generation,
                    lsn: request.lsn,
                })
                .await?;
            Ok(StreamFetchResponse { frame })
        }
    })
}

/// The stream progress query handler (the promotion gate's check).
fn register_stream_progress_handler(
    router: ServiceRouter,
    service: Arc<ControlPlaneService>,
    directory: Arc<ClusterDirectory>,
) -> RS<ServiceRouter> {
    router.handle::<StreamProgressRequest, _, _>(move |request, ctx| {
        let service = service.clone();
        let directory = directory.clone();
        async move {
            // Only group members may query stream progress.
            let _from = directory.node_of_worker(ctx.src()).ok_or_else(|| {
                mudu_error!(
                    ErrorCode::ObjectNotFound,
                    format!("stream progress query from unknown endpoint {}", ctx.src())
                )
            })?;
            let progress = service
                .peer_progress_at(mudu_raft::stream::StreamId(request.stream_id), request.lsn)
                .await?;
            Ok(StreamProgressResponse {
                progress: progress.map(|progress| {
                    ::catalog::contract::control_plane::StreamProgressWire {
                        stream_id: progress.stream_id.0,
                        stream_generation: progress.stream_generation,
                        end_lsn: progress.end_lsn,
                        segment_digest: progress.segment_digest,
                    }
                }),
            })
        }
    })
}

/// Forwarded proposals (tell-only, arrive at the control leader): submit to
/// the meta stream; the outcome reaches the originator through the committed
/// stream and its local apply.
fn register_forward_handler(channel: &Channel, service: Arc<ControlPlaneService>) -> RS<()> {
    channel.raw().register_kind_handler(
        CLUSTER_META_FORWARD,
        Arc::new(move |envelope: Envelope| {
            let service = service.clone();
            Box::pin(async move {
                let forward: MetaGroupForward = rmp_serde::from_slice(envelope.payload())
                    .map_err(|e| mudu_error!(ErrorCode::Decode, "decode meta forward error", e))?;
                if service.is_leader() {
                    if let Err(err) = service.submit_meta(forward.request_id, forward.op).await {
                        tracing::trace!("forwarded meta submit failed: {}", err);
                    }
                }
                Ok(())
            })
        }),
    )
}

/// Drives one proposal on this node: locally on the control leader,
/// forwarded to the leader otherwise. Returns after the committed op applied
/// locally — so a follower's client sees its own DDL once its own catalog
/// has it.
async fn drive_meta_proposal(
    service: Arc<ControlPlaneService>,
    channel: Channel,
    request_id: u128,
    op: MetaOp,
) -> MetaGroupRpcResponse {
    if service.is_leader() {
        if let Err(err) = service.submit_meta(request_id, op).await {
            if !service.is_leader() {
                return MetaGroupRpcResponse::NotLeader {
                    leader: service.leader(),
                    term: service.term(),
                };
            }
            return MetaGroupRpcResponse::Err(err.to_string());
        }
    } else {
        // Forward to the current control leader (fire-and-forget); the
        // outcome flows back through the replicated stream and this node's
        // local apply.
        match service.leader() {
            Some(leader) => {
                let forwarded = channel
                    .endpoint::<MetaGroupForward>(derive_node_endpoint(leader))
                    .tell(MetaGroupForward { request_id, op })
                    .await;
                if let Err(err) = forwarded {
                    return MetaGroupRpcResponse::Err(err.to_string());
                }
            }
            None => {
                return MetaGroupRpcResponse::NotLeader {
                    leader: None,
                    term: service.term(),
                };
            }
        }
    }
    match service
        .wait_meta_result(request_id, META_APPLY_TIMEOUT)
        .await
    {
        Ok(()) => MetaGroupRpcResponse::Applied,
        Err(err) => MetaGroupRpcResponse::Err(err.to_string()),
    }
}

/// Registers the epoch heartbeat handler (node → control leader tells).
fn register_epoch_heartbeat_handler(
    channel: &Channel,
    service: Arc<ControlPlaneService>,
) -> RS<()> {
    channel.raw().register_kind_handler(
        CLUSTER_EPOCH_TELL,
        Arc::new(move |envelope: Envelope| {
            let service = service.clone();
            Box::pin(async move {
                let heartbeat: EpochHeartbeat =
                    rmp_serde::from_slice(envelope.payload()).map_err(|e| {
                        mudu_error!(ErrorCode::Decode, "decode epoch heartbeat error", e)
                    })?;
                if *envelope.src() != derive_node_endpoint(heartbeat.node_id) {
                    return Err(mudu_error!(
                        ErrorCode::InvalidArgument,
                        "epoch report source mismatch"
                    ));
                }
                service.note_drained(heartbeat);
                Ok(())
            })
        }),
    )
}

/// The heartbeat reporter: periodically reads the node's drained epoch from
/// its oracle and tells the control-plane leader.
fn spawn_epoch_reporter(
    node_id: u64,
    service: Arc<ControlPlaneService>,
    transport: Arc<ClusterTransport>,
    oracle: Arc<xcontract::epoch_oracle::ClusterEpochOracle>,
) -> RS<(StopTx, SJoinHandle<RS<()>>)> {
    let (stop_tx, stop_rx) = stop_channel();
    let handle =
        mudu_sys::task::sync::spawn_thread_named("epoch-heartbeat".to_string(), move || {
            loop {
                mudu_sys::task::sync::sleep_blocking(EPOCH_HEARTBEAT_INTERVAL);
                if stop_rx.is_stopped() {
                    break;
                }
                let status = service.raft().status();
                let Some(leader) = status.leader_id else {
                    continue;
                };
                let (closed_epoch, drained_epoch) = oracle.closed_drained_epochs()?;
                let heartbeat = EpochHeartbeat {
                    node_id,
                    control_term: status.term,
                    closed_epoch,
                    drained_epoch,
                };
                if leader == node_id {
                    service.note_drained(heartbeat);
                    continue;
                }
                let payload = match rmp_serde::to_vec(&heartbeat) {
                    Ok(payload) => payload,
                    Err(_) => continue,
                };
                let envelope = Envelope::new(
                    0,
                    None,
                    derive_node_endpoint(node_id),
                    derive_node_endpoint(leader),
                    CLUSTER_EPOCH_TELL,
                    payload,
                    DeliveryMode::FireAndForget,
                );
                let _ = transport.send_envelope(envelope);
            }
            Ok(())
        })?;
    Ok((stop_tx, handle))
}
