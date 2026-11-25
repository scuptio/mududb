use super::super::{
    CommitVecTopology, PartitionRouteEntry, PartitionRouteRequest, PartitionRouteResponse,
    RaftStatusResponse, ReplicationCountersResponse, ReplicationStorageResponse,
    ReplicationTopology, ServerReplicationResponse, ServerTopology, SnapshotStatusResponse,
    StreamStatusResponse, SubscriberLagResponse, WorkerReplicationResponse, WorkerTopology,
};
use super::KernelHttpApi;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use std::ops::Bound;

/// Topology, replication-status, and partition-routing endpoints.
impl KernelHttpApi {
    pub(super) async fn server_topology(&self) -> RS<ServerTopology> {
        let replication = self
            .raft_node_id
            .and_then(::repl::raft_share_for_node)
            .map(|share| {
                let mut commit_vec: Vec<CommitVecTopology> = share
                    .committed_vector()
                    .into_iter()
                    .map(|(worker_index, (lsn, hash))| CommitVecTopology {
                        worker_index,
                        lsn,
                        seg_hash: ::repl::seg_hash::seg_hash_hex(&hash),
                    })
                    .collect();
                commit_vec.sort_by_key(|entry| entry.worker_index);
                ReplicationTopology {
                    node_id: share.node_id(),
                    role: format!("{:?}", share.role()),
                    term: share.term(),
                    leader_node_id: share.leader_id(),
                    serving: share.is_serving(),
                    commit_index: share.commit_index(),
                    commit_vec,
                }
            });
        Ok(ServerTopology {
            worker_count: self.worker_registry.workers().len(),
            tcp_multi_port: self.tcp_multi_port,
            tcp_base_listen_port: self.tcp_base_listen_port,
            replication,
            workers: self
                .worker_registry
                .workers()
                .iter()
                .map(|worker| WorkerTopology {
                    worker_index: worker.worker_index,
                    tcp_listen_port: if self.tcp_multi_port {
                        self.tcp_base_listen_port
                            .saturating_add(worker.worker_index as u16)
                    } else {
                        self.tcp_base_listen_port
                    },
                    worker_id: worker.worker_id,
                    partitions: worker.partition_ids.clone(),
                })
                .collect(),
        })
    }

    pub(super) async fn server_replication(&self) -> RS<ServerReplicationResponse> {
        let Some(node_id) = self.raft_node_id else {
            // No replication configured: the endpoint answers with an
            // explicit single-node marker instead of an error.
            return Ok(ServerReplicationResponse {
                mode: "none".to_string(),
                node_id: None,
                role: None,
                serving: true,
                raft: None,
                stream: None,
                workers: Vec::new(),
                subscribers: Vec::new(),
                storage: ReplicationStorageResponse::default(),
                counters: ReplicationCountersResponse::default(),
            });
        };
        let status = ::repl::status::collect_replication_status(node_id)?.ok_or_else(|| {
            mudu_error!(
                ErrorCode::InvalidState,
                "replication status is not available on this node"
            )
        })?;
        Ok(ServerReplicationResponse {
            mode: status.mode,
            node_id: Some(status.node_id),
            role: Some(status.role),
            serving: status.serving,
            raft: status.raft.map(|raft| RaftStatusResponse {
                term: raft.term,
                leader_node_id: raft.leader_node_id,
                commit_index: raft.commit_index,
                commit_vec: raft
                    .commit_vec
                    .into_iter()
                    .map(|(worker_index, lsn, seg_hash)| CommitVecTopology {
                        worker_index,
                        lsn,
                        seg_hash,
                    })
                    .collect(),
                voters: raft.voters,
                learners: raft.learners,
                conf_version_committed: raft.conf_version_committed,
                conf_version_new: raft.conf_version_new,
            }),
            stream: status.stream.map(|stream| StreamStatusResponse {
                epoch: stream.epoch,
                primary_node_id: stream.primary_node_id,
                is_primary: stream.is_primary,
                sync: stream.sync,
                sync_timeout_ms: stream.sync_timeout_ms,
            }),
            workers: status
                .workers
                .into_iter()
                .map(|worker| WorkerReplicationResponse {
                    worker_index: worker.worker_index,
                    wal_next_lsn: worker.wal_next_lsn,
                    chain_lsn: worker.chain_lsn,
                    chain_seg_hash: worker.chain_seg_hash,
                    staging_max_lsn: worker.staging_max_lsn,
                    snapshot: worker.snapshot.map(|snapshot| SnapshotStatusResponse {
                        committed_lsn: snapshot.committed_lsn,
                        taken_at_ms: snapshot.taken_at_ms,
                        dir_bytes: snapshot.dir_bytes,
                    }),
                })
                .collect(),
            subscribers: status
                .subscribers
                .into_iter()
                .map(|subscriber| SubscriberLagResponse {
                    node_id: subscriber.node_id,
                    worker_index: subscriber.worker_index,
                    stored_lsn: subscriber.stored_lsn,
                    source_lsn: subscriber.source_lsn,
                    lag_lsn: subscriber.lag_lsn,
                })
                .collect(),
            storage: ReplicationStorageResponse {
                wal_bytes: status.storage.wal_bytes,
                staging_bytes: status.storage.staging_bytes,
                raft_log_bytes: status.storage.raft_log_bytes,
                snapshot_bytes: status.storage.snapshot_bytes,
                refreshed_at_ms: status.storage.refreshed_at_ms,
            },
            counters: ReplicationCountersResponse {
                sync_write_timeouts: status.counters.sync_write_timeouts,
                snapshots_taken: status.counters.snapshots_taken,
                wal_chunks_reclaimed: status.counters.wal_chunks_reclaimed,
                snapshot_offers_served: status.counters.snapshot_offers_served,
                snapshot_restores_completed: status.counters.snapshot_restores_completed,
            },
        })
    }

    pub(super) async fn route_partition(
        &self,
        request: PartitionRouteRequest,
    ) -> RS<PartitionRouteResponse> {
        let rule = self
            .meta_mgr
            .get_partition_rule_by_name(&request.rule_name)
            .await?
            .ok_or_else(|| {
                mudu::mudu_error!(
                    mudu::error::ErrorCode::ObjectNotFound,
                    format!("no such partition rule {}", request.rule_name)
                )
            })?;

        // The live cluster directory of this server, when the kernel runs in
        // cluster mode (registered at startup; `None` for single-group).
        let directory = self
            .server_instance_id
            .and_then(::server::server::cluster_directory::cluster_directory_for);

        let partition_ids = self.resolve_route_request_partitions(&rule, request)?;

        let mut routes = Vec::with_capacity(partition_ids.len());
        for partition_id in partition_ids {
            let (group_id, node_id, client_addr, cluster_worker) =
                Self::cluster_route_entry(&rule, partition_id, directory.as_deref());
            // Cluster mode resolves the worker through the directory (the
            // catalog placement only exists in single-group mode).
            let worker_id = match cluster_worker {
                Some(worker) => worker,
                None => self.resolve_partition_worker(partition_id).await?,
            };
            routes.push(PartitionRouteEntry {
                partition_id,
                worker_id,
                group_id,
                node_id,
                client_addr,
            });
        }
        Ok(PartitionRouteResponse {
            routes,
            rule_version: rule.version,
            topology_version: directory
                .as_ref()
                .map(|directory| {
                    use ::catalog::contract::cluster_router::ClusterRouter;
                    directory.topology_version()
                })
                .unwrap_or(0),
            placement_version: directory
                .as_ref()
                .map(|directory| {
                    use ::catalog::contract::cluster_router::ClusterRouter;
                    directory.placement_version()
                })
                .unwrap_or(0),
        })
    }

    /// Resolves the request's key/range form to partition ids.
    fn resolve_route_request_partitions(
        &self,
        rule: &::catalog::contract::partition_rule::PartitionRuleDesc,
        request: PartitionRouteRequest,
    ) -> RS<Vec<mudu::common::id::OID>> {
        if let Some(keys) = request.keys {
            if request.key.is_some() || request.start.is_some() || request.end.is_some() {
                return Err(mudu::mudu_error!(
                    mudu::error::ErrorCode::Parse,
                    "partition route request cannot mix `keys` with `key`/`start`/`end`"
                ));
            }
            // Batch point lookups: exactly one partition per key, in order.
            let mut ids = Vec::with_capacity(keys.len());
            for key in keys {
                ids.push(
                    self.partition_router.route_rule_exact_partition(
                        rule,
                        &key.into_iter()
                            .map(|value| value.into_bytes())
                            .collect::<Vec<_>>(),
                    )?,
                );
            }
            return Ok(ids);
        }
        if let Some(key) = request.key {
            if request.start.is_some() || request.end.is_some() {
                return Err(mudu::mudu_error!(
                    mudu::error::ErrorCode::Parse,
                    "partition route request cannot specify both key and range"
                ));
            }
            return Ok(vec![
                self.partition_router.route_rule_exact_partition(
                    rule,
                    &key.into_iter()
                        .map(|value| value.into_bytes())
                        .collect::<Vec<_>>(),
                )?,
            ]);
        }
        let start = match request.start {
            Some(values) => Bound::Included(
                values
                    .into_iter()
                    .map(|value| value.into_bytes())
                    .collect::<Vec<_>>(),
            ),
            None => Bound::Unbounded,
        };
        let end = match request.end {
            Some(values) => Bound::Excluded(
                values
                    .into_iter()
                    .map(|value| value.into_bytes())
                    .collect::<Vec<_>>(),
            ),
            None => Bound::Unbounded,
        };
        self.partition_router
            .route_rule_range_partitions(rule, &start, &end)
    }

    /// Cluster-mode route detail of one partition: owning group, its serving
    /// node, the node's client address (for direct client routing), and the
    /// worker endpoint id derived from the serving node.
    fn cluster_route_entry(
        rule: &::catalog::contract::partition_rule::PartitionRuleDesc,
        partition_id: mudu::common::id::OID,
        directory: Option<&::server::server::cluster_directory::ClusterDirectory>,
    ) -> (
        Option<u64>,
        Option<u64>,
        Option<String>,
        Option<mudu::common::id::OID>,
    ) {
        use ::catalog::contract::cluster_router::ClusterRouter;
        let Some(directory) = directory else {
            return (None, None, None, None);
        };
        let Some(def) = rule
            .partitions
            .iter()
            .find(|partition| partition.partition_id == partition_id)
        else {
            return (None, None, None, None);
        };
        let Some((group_id, worker_index)) = directory.placement(&rule.name, &def.name) else {
            return (None, None, None, None);
        };
        let node_id = directory.serving_node(group_id);
        let client_addr = node_id
            .and_then(|node| directory.node_client_addr(node))
            .map(|(host, port)| format!("{}:{}", host, port));
        let worker_id = directory.resolve_group_worker(group_id, worker_index).ok();
        (Some(group_id), node_id, client_addr, worker_id)
    }
}
