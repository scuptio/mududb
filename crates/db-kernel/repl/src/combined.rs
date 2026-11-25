//! The combined control + data shared group: one replication group carrying
//! the meta/epoch catalog streams, the per-worker data streams and the
//! tx_decision stream. This module knows the stream descriptors and adapter
//! capabilities of both planes so the server assembly can open one shared
//! group in the Combined deployment.

use crate::data_plane::{data_stream_id, tx_decision_descriptor};
use crate::log_stream::{AdapterCapabilities, SharedGroup, StreamTransport};
use mudu::common::result::RS;
use mudu_raft::stream::{GroupIdobject, StreamDescriptor};
use mudu_raft::RaftHandle;
use std::path::Path;
use std::sync::Arc;

/// The descriptors of every stream of a combined control + data group.
pub fn combined_stream_descriptors(worker_count: u32) -> Vec<StreamDescriptor> {
    let mut descriptors = vec![
        crate::control_plane::streams::meta_stream_descriptor(),
        crate::control_plane::streams::epoch_stream_descriptor(),
    ];
    for worker_index in 0..worker_count {
        descriptors.push(crate::data_plane::data_stream_descriptor(data_stream_id(
            worker_index,
        )));
    }
    descriptors.push(tx_decision_descriptor());
    descriptors
}

/// The adapter capabilities of a combined control + data group.
pub fn combined_adapter_capabilities(worker_count: u32) -> AdapterCapabilities {
    let mut capabilities = AdapterCapabilities::default();
    for descriptor in combined_stream_descriptors(worker_count) {
        capabilities.add(
            descriptor.kind,
            descriptor.codec_version,
            descriptor.digest_version,
        );
    }
    capabilities
}

/// Opens the combined control + data shared group on an existing Raft handle
/// (the Combined deployment's single replication group). `bootstrap_stop`
/// aborts the stream registration wait promptly on server shutdown.
pub async fn open_combined_shared_group(
    raft: Arc<RaftHandle>,
    identity: GroupIdobject,
    worker_count: u32,
    log_dir: &Path,
    transport: Arc<dyn StreamTransport>,
    bootstrap_stop: Option<mudu_sys::sync::async_::stop_flag::StopRx>,
) -> RS<SharedGroup> {
    let capabilities = combined_adapter_capabilities(worker_count);
    let descriptors = combined_stream_descriptors(worker_count);
    SharedGroup::open(
        raft,
        identity,
        capabilities,
        descriptors,
        log_dir,
        transport,
        bootstrap_stop,
    )
    .await
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use crate::control_plane::MetaApplier;
    use crate::control_plane::{ControlPlaneCfg, ControlPlaneService};
    use crate::data_plane::{DataPlaneCfg, DataPlaneService};
    use crate::log_stream::{
        AckRound, DurableAck, DurableStreamLog, GenericReplicationGroup, StreamFetch, StreamSession,
    };
    use crate::raft_service::RaftShare;
    use async_trait::async_trait;
    use catalog::contract::epoch_oracle::ClusterEpochState;
    use catalog::contract::meta_op::MetaOp;
    use mudu::common::result::RS;
    use mudu::error::ErrorCode;
    use mudu::mudu_error;
    use mudu_raft::stream::{GroupId, StreamId, StreamProgress};
    use mudu_raft::{AdmissionGuard, InMemoryNetwork, RaftCfg, RaftHandle, RaftRole};
    use mudu_sys::sync::SMutex;
    use std::collections::BTreeMap;
    use std::sync::atomic::AtomicBool;
    use std::sync::{Arc, Weak};
    use std::time::Duration;

    const TIMEOUT: Duration = Duration::from_secs(15);

    fn identity() -> GroupIdobject {
        GroupIdobject {
            cluster_id: 7,
            group_id: GroupId(1),
            generation: 1,
        }
    }

    /// A transport that never reaches a peer: sufficient for a single node.
    struct NoPeers;
    #[async_trait]
    impl StreamTransport for NoPeers {
        async fn session(&self, _: u64) -> RS<StreamSession> {
            Err(mudu_error!(ErrorCode::Network, "no peer transport"))
        }
        async fn push(
            &self,
            _: u64,
            _: &StreamSession,
            _: &AckRound,
            _: Vec<u8>,
        ) -> RS<Option<DurableAck>> {
            Err(mudu_error!(ErrorCode::Network, "no peer transport"))
        }
        async fn fetch(&self, _: u64, _: &StreamSession, _: StreamFetch) -> RS<Vec<u8>> {
            Err(mudu_error!(ErrorCode::Network, "no peer transport"))
        }
        async fn progress_at(
            &self,
            _: u64,
            _: &StreamSession,
            _: StreamId,
            _: u64,
        ) -> RS<Option<StreamProgress>> {
            Err(mudu_error!(ErrorCode::Network, "no peer transport"))
        }
    }

    /// A no-op catalog applier: sufficient to exercise the meta apply driver.
    struct NoopApplier;
    #[async_trait]
    impl MetaApplier for NoopApplier {
        async fn validate_recovery(
            &self,
            _log: &DurableStreamLog,
            _applied: Option<&StreamProgress>,
            _committed: Option<&StreamProgress>,
        ) -> RS<()> {
            Ok(())
        }
        async fn apply(&self, _op: MetaOp, _version: u64) -> RS<()> {
            Ok(())
        }
    }

    #[test]
    fn combined_assembly_attaches_both_services_to_one_shared_group() {
        let dir = mudu_sys::env_var::temp_dir()
            .join(format!("combined-assembly-{}", mudu_sys::random::uuid_v4()));
        let runtime = mudu_sys::task::async_::build_current_thread_runtime().unwrap();
        let (control, data) = runtime.block_on(async {
            let raft = Arc::new(
                RaftHandle::start_async(
                    RaftCfg::new(1, vec![1], 25, 4, dir.join("raft")),
                    InMemoryNetwork::new().transport(1),
                )
                .await
                .unwrap(),
            );
            let shared = Arc::new(
                open_combined_shared_group(
                    raft,
                    identity(),
                    1,
                    &dir.join("shared"),
                    Arc::new(NoPeers),
                    None,
                )
                .await
                .unwrap(),
            );
            // The combined group carries the four stream kinds.
            let state = shared.registry.refresh().await.unwrap();
            assert_eq!(state.streams().len(), 4);
            assert!(state.streams().contains_key(&StreamId(1)));
            assert!(state.streams().contains_key(&StreamId(2)));
            assert!(state.streams().contains_key(&StreamId(3)));
            assert!(state
                .streams()
                .contains_key(&crate::data_plane::data_stream_id(0)));

            let control_cfg = ControlPlaneCfg {
                node_id: 1,
                voters: vec![1],
                learners: vec![],
                epoch_participants: vec![1],
                log_dir: dir.join("control"),
                tick_ms: 25,
                election_timeout_ticks: 4,
                advance_interval: Duration::from_millis(25),
                identity: identity(),
                epoch_state: ClusterEpochState::new(),
                control_gate: Arc::new(AtomicBool::new(false)),
                sparse_epoch: false,
                bootstrap_stop: None,
            };
            let control = ControlPlaneService::start_on_shared_group(
                control_cfg,
                shared.clone(),
                Arc::new(NoopApplier),
            )
            .await
            .unwrap();

            let data_cfg = DataPlaneCfg {
                node_id: 1,
                identity: identity(),
                worker_count: 1,
                log_dir: dir.join("data"),
                snapshot_threshold_bytes: 1024 * 1024,
                share: Arc::new(RaftShare::new(1, vec![], 0)),
                bootstrap_stop: None,
            };
            let data = DataPlaneService::open_on_shared_group(data_cfg, shared)
                .await
                .unwrap();
            (control, data)
        });
        // The control plane operates through the shared group.
        runtime.block_on(async {
            control
                .submit_meta(
                    1,
                    MetaOp::DropFsType {
                        name: "x".to_string(),
                    },
                )
                .await
                .unwrap();
            control.wait_meta_result(1, TIMEOUT).await.unwrap();
            assert!(control.is_synced().await.unwrap());
        });
        // Stop the control plane first (it stops the shared group); the data
        // plane then re-stops the already-stopped group, so tolerate its error.
        control.stop().unwrap();
        data.stop().ok();
        mudu_sys::fs::sync::remove_dir_all(&dir).unwrap();
    }

    /// An in-memory stream transport routing push/fetch to the peer's shared
    /// replication group.
    struct GroupNet {
        groups: SMutex<BTreeMap<u64, Weak<GenericReplicationGroup>>>,
    }
    impl Default for GroupNet {
        fn default() -> Self {
            Self {
                groups: SMutex::new(BTreeMap::new()),
            }
        }
    }
    struct Link {
        from: u64,
        net: Arc<GroupNet>,
    }
    impl Link {
        async fn peer(&self, peer: u64) -> RS<Arc<GenericReplicationGroup>> {
            self.net
                .groups
                .lock()
                .map_err(|_| mudu_error!(ErrorCode::Internal, "group net lock poisoned"))?
                .get(&peer)
                .and_then(Weak::upgrade)
                .ok_or_else(|| mudu_error!(ErrorCode::Network, "group peer unavailable"))
        }
    }
    #[async_trait]
    impl StreamTransport for Link {
        async fn session(&self, peer: u64) -> RS<StreamSession> {
            self.peer(peer).await?;
            Ok(StreamSession {
                id: ((self.from as u128) << 64) | peer as u128,
                valid: AdmissionGuard::new(),
            })
        }
        async fn push(
            &self,
            peer: u64,
            session: &StreamSession,
            round: &AckRound,
            frame: Vec<u8>,
        ) -> RS<Option<DurableAck>> {
            self.peer(peer)
                .await?
                .receive(self.from, session, round, &frame)
                .await
        }
        async fn fetch(&self, peer: u64, _: &StreamSession, request: StreamFetch) -> RS<Vec<u8>> {
            self.peer(peer).await?.fetch(request).await
        }
        async fn progress_at(
            &self,
            peer: u64,
            _: &StreamSession,
            stream_id: StreamId,
            lsn: u64,
        ) -> RS<Option<StreamProgress>> {
            self.peer(peer)
                .await?
                .peer_progress_at(stream_id, lsn)
                .await
        }
    }

    async fn wait_leader(rafts: &[(u64, Arc<RaftHandle>)]) -> u64 {
        mudu_sys::timeout(TIMEOUT, async {
            loop {
                for (id, raft) in rafts {
                    if raft.status().role == RaftRole::Leader {
                        return *id;
                    }
                }
                mudu_sys::task::async_::sleep(Duration::from_millis(5))
                    .await
                    .unwrap();
            }
        })
        .await
        .unwrap()
    }

    /// Commits one catalog op through the leader and asserts it applies on
    /// every replica's local catalog.
    async fn submit_and_verify_catalog(services: &[(u64, Arc<ControlPlaneService>)], leader: u64) {
        let leader_service = services
            .iter()
            .find(|(id, _)| *id == leader)
            .map(|(_, s)| s.clone())
            .unwrap();
        leader_service
            .submit_meta(
                1,
                MetaOp::DropFsType {
                    name: "ha".to_string(),
                },
            )
            .await
            .unwrap();
        for (_, service) in services {
            service.wait_meta_result(1, TIMEOUT).await.unwrap();
            assert!(service.is_synced().await.unwrap());
        }
    }

    #[test]
    fn combined_ha_shares_election_and_replicates_the_catalog() {
        combined_ha_shares_election_and_replicates_the_catalog_impl(false);
    }

    /// The same Combined HA scenario on the sparse epoch path (P4).
    #[test]
    fn combined_ha_shares_election_and_replicates_the_catalog_sparse() {
        combined_ha_shares_election_and_replicates_the_catalog_impl(true);
    }

    fn combined_ha_shares_election_and_replicates_the_catalog_impl(sparse_epoch: bool) {
        let _serial = crate::control_plane::test_guard();
        let dir = mudu_sys::env_var::temp_dir()
            .join(format!("combined-ha-{}", mudu_sys::random::uuid_v4()));
        let runtime = mudu_sys::task::async_::build_current_thread_runtime().unwrap();
        let (rafts, services) = runtime.block_on(async {
            let raft_net = InMemoryNetwork::new();
            let mut rafts = Vec::new();
            for node_id in 1..=3u64 {
                let cfg = RaftCfg::new(
                    node_id,
                    vec![1, 2, 3],
                    25,
                    4,
                    dir.join(node_id.to_string()).join("raft"),
                )
                .with_election_timeout_offset(node_id);
                let raft = Arc::new(
                    RaftHandle::start_async(cfg, raft_net.transport(node_id))
                        .await
                        .unwrap(),
                );
                raft_net.register(node_id, raft.input_sender()).unwrap();
                rafts.push((node_id, raft));
            }
            let leader = wait_leader(&rafts).await;

            // Every node opens its own replica of the shared group over the
            // same Raft group, then attaches its control plane.
            let group_net = Arc::new(GroupNet::default());
            let mut services = Vec::new();
            for (node_id, raft) in &rafts {
                let shared = Arc::new(
                    open_combined_shared_group(
                        raft.clone(),
                        identity(),
                        1,
                        &dir.join(node_id.to_string()).join("shared"),
                        Arc::new(Link {
                            from: *node_id,
                            net: group_net.clone(),
                        }),
                        None,
                    )
                    .await
                    .unwrap(),
                );
                group_net
                    .groups
                    .lock()
                    .unwrap()
                    .insert(*node_id, Arc::downgrade(&shared.group));
                let cfg = ControlPlaneCfg {
                    node_id: *node_id,
                    voters: vec![1, 2, 3],
                    learners: vec![],
                    epoch_participants: vec![1, 2, 3],
                    log_dir: dir.join(node_id.to_string()).join("control"),
                    tick_ms: 25,
                    election_timeout_ticks: 4,
                    advance_interval: Duration::from_millis(25),
                    identity: identity(),
                    epoch_state: ClusterEpochState::new(),
                    control_gate: Arc::new(AtomicBool::new(false)),
                    sparse_epoch,
                    bootstrap_stop: None,
                };
                let service =
                    ControlPlaneService::start_on_shared_group(cfg, shared, Arc::new(NoopApplier))
                        .await
                        .unwrap();
                services.push((*node_id, service));
            }
            // One catalog op committed by the leader applies on every replica
            // through the shared group.
            submit_and_verify_catalog(&services, leader).await;
            (rafts, services)
        });
        // Clean up: stop every replica; the shared raft may already be stopped.
        for (_, service) in services {
            service.stop().ok();
        }
        drop(rafts);
        mudu_sys::fs::sync::remove_dir_all(&dir).unwrap();
    }
}
