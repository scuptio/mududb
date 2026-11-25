//! Cluster-mode partition routing of the worker contract (the data-plane
//! half): partition id ->
//! static placement -> `(group, worker_index)` -> the endpoint id of that
//! worker on the group's serving node, plus the serving check inbound
//! partition RPCs run before any execution.

use super::*;
use crate::partition_rpc::PartitionRpcRequest;
use crate::partition_rpc::PartitionRpcResponse;
use ::catalog::contract::cluster_router::ClusterRouter;
use ::catalog::contract::worker_snapshot::WorkerSnapshotMgr;
use ::catalog::repl_ctrl::{RaftShareView, ReplControl};

/// Write-side fence (phase 3 splits/merges): waits out the migration
/// window, then verifies the partition is not migrating.
pub(crate) async fn ensure_partition_write_open(
    cluster_router: &Option<Arc<dyn ClusterRouter>>,
    meta_mgr: &Arc<dyn MetaMgr>,
    partition_id: OID,
) -> RS<()> {
    ensure_partition_write_open_nowait(cluster_router, meta_mgr, partition_id).await
}

/// The non-waiting form for the commit path: a staged write of a migrating
/// partition aborts the transaction (the client retries the whole
/// transaction).
pub(crate) async fn ensure_partition_write_open_nowait(
    cluster_router: &Option<Arc<dyn ClusterRouter>>,
    meta_mgr: &Arc<dyn MetaMgr>,
    partition_id: OID,
) -> RS<()> {
    let Some(router) = cluster_router else {
        return Ok(());
    };
    if partition_id == DEFAULT_UNPARTITIONED_TABLE_PARTITION_ID {
        return Ok(());
    }
    let Some((rule, partition)) = meta_mgr.find_partition_names(partition_id).await? else {
        return Ok(());
    };
    if matches!(
        router.migration_state(&rule, &partition),
        ::catalog::contract::cluster_router::MigrationState::Migrating { .. }
    ) {
        return Err(mudu_error!(
            ErrorCode::ResourceBusy,
            format!("partition {partition_id} is migrating; retry the transaction")
        ));
    }
    Ok(())
}

/// Read-side fence (phase 3 splits/merges): while a re-homing
/// split/merge runs (`fence_reads`), reads of the partition fail fast
/// with `ResourceBusy` (same no-wait rule as the write fence).
pub(crate) async fn ensure_partition_read_open(
    cluster_router: &Option<Arc<dyn ClusterRouter>>,
    meta_mgr: &Arc<dyn MetaMgr>,
    partition_id: OID,
) -> RS<()> {
    let Some(router) = cluster_router else {
        return Ok(());
    };
    if partition_id == DEFAULT_UNPARTITIONED_TABLE_PARTITION_ID {
        return Ok(());
    }
    let Some((rule, partition)) = meta_mgr.find_partition_names(partition_id).await? else {
        return Ok(());
    };
    if matches!(
        router.migration_state(&rule, &partition),
        ::catalog::contract::cluster_router::MigrationState::Migrating {
            fence_reads: true,
            ..
        }
    ) {
        return Err(mudu_error!(
            ErrorCode::ResourceBusy,
            format!("partition {partition_id} is re-homing (split/merge); retry the transaction")
        ));
    }
    Ok(())
}

/// Resolves the worker owning `partition_id`: cluster placement in cluster
/// mode, otherwise the catalog's placement (falling back to the default
/// unpartitioned worker).
pub(crate) async fn resolve_partition_worker(
    cluster_router: &Option<Arc<dyn ClusterRouter>>,
    meta_mgr: &Arc<dyn MetaMgr>,
    default_unpartitioned_worker_id: OID,
    partition_id: OID,
) -> RS<Option<OID>> {
    if cluster_router.is_some() {
        return resolve_cluster_partition_worker(
            cluster_router,
            meta_mgr,
            default_unpartitioned_worker_id,
            partition_id,
        )
        .await;
    }
    match meta_mgr.get_partition_worker(partition_id).await? {
        Some(worker_id) => Ok(Some(worker_id)),
        None if partition_id == DEFAULT_UNPARTITIONED_TABLE_PARTITION_ID => {
            Ok((default_unpartitioned_worker_id != 0).then_some(default_unpartitioned_worker_id))
        }
        None => Ok(None),
    }
}

/// Cluster-mode resolution: partition id → static placement (via the
/// catalog's rule/partition names) → `(group, worker_index)` → the
/// endpoint id of that worker on the group's serving node.
async fn resolve_cluster_partition_worker(
    cluster_router: &Option<Arc<dyn ClusterRouter>>,
    meta_mgr: &Arc<dyn MetaMgr>,
    default_unpartitioned_worker_id: OID,
    partition_id: OID,
) -> RS<Option<OID>> {
    let Some(router) = cluster_router else {
        return Err(mudu_error!(
            ErrorCode::InvalidState,
            "cluster resolution without a cluster router"
        ));
    };
    if partition_id == DEFAULT_UNPARTITIONED_TABLE_PARTITION_ID {
        return Ok(
            (default_unpartitioned_worker_id != 0).then_some(default_unpartitioned_worker_id)
        );
    }
    let Some((rule_name, partition_name)) = meta_mgr.find_partition_names(partition_id).await?
    else {
        return Err(mudu_error!(
            ErrorCode::ObjectNotFound,
            format!(
                "partition {} is not defined by any partition rule",
                partition_id
            )
        ));
    };
    let Some((group_id, worker_index)) = router.placement(&rule_name, &partition_name) else {
        return Err(mudu_error!(
            ErrorCode::NotImplemented,
            format!(
                "partition {}.{} has no cluster placement",
                rule_name, partition_name
            )
        ));
    };
    // Cross-group RPC carries oids verbatim; only name-derived oids are
    // identical on every group, so reject cross-group routing of
    // pre-existing random-id partitions.
    if group_id != router.self_group_id()
        && partition_id
            != ::catalog::contract::derive_oid::derive_partition_oid(&rule_name, &partition_name)
    {
        return Err(mudu_error!(
            ErrorCode::NotImplemented,
            format!(
                "partition {}.{} has a random oid and cannot be routed across node groups; recreate the partition rule to enable cross-group routing",
                rule_name, partition_name
            )
        ));
    }
    Ok(Some(router.resolve_group_worker(group_id, worker_index)?))
}

/// Cluster mode: the node group owning `partition_id` per the static
/// topology; `Ok(None)` when not in cluster mode or the partition has
/// no placement.
pub(crate) async fn resolve_partition_group(
    cluster_router: &Option<Arc<dyn ClusterRouter>>,
    meta_mgr: &Arc<dyn MetaMgr>,
    partition_id: OID,
) -> RS<Option<u64>> {
    let Some(router) = cluster_router else {
        return Ok(None);
    };
    if partition_id == DEFAULT_UNPARTITIONED_TABLE_PARTITION_ID {
        return Ok(Some(router.self_group_id()));
    }
    let Some((rule, partition)) = meta_mgr.find_partition_names(partition_id).await? else {
        return Ok(None);
    };
    Ok(router
        .placement(&rule, &partition)
        .map(|(group_id, _)| group_id))
}

/// Follower reads (phase 4): a caught-up replica serves stable-epoch
/// snapshot reads instead of redirecting — its applied commit watermark
/// has reached the cluster stable epoch, so the read sees exactly the
/// committed prefix the leader would answer.
pub(crate) fn cluster_non_serving_redirect(
    cluster_router: &Option<Arc<dyn ClusterRouter>>,
    snapshot_mgr: &WorkerSnapshotMgr,
    raft_share: &SMutex<Option<Arc<dyn RaftShareView>>>,
    repl_ctl: &SMutex<Option<Arc<dyn ReplControl>>>,
    request: &PartitionRpcRequest,
) -> RS<Option<Option<u64>>> {
    let Some(router) = cluster_router else {
        return Ok(None);
    };
    let follower_read = matches!(
        request,
        PartitionRpcRequest::ReadKey { .. } | PartitionRpcRequest::ReadRange { .. }
    ) && replica_caught_up(snapshot_mgr);
    if follower_read {
        return Ok(None);
    }
    if let Some(share) = raft_share
        .lock()
        .map_err(|_| mudu_error!(ErrorCode::Internal, "raft share lock poisoned"))?
        .clone()
    {
        if share.is_serving_leader() {
            return Ok(None);
        }
        // During the leader's own heal window the leader is self: report
        // "unknown" rather than a self-redirect loop.
        return Ok(Some(
            share
                .leader_node_id()
                .filter(|leader| *leader != router.self_node_id()),
        ));
    }
    if let Some(ctl) = repl_ctl
        .lock()
        .map_err(|_| mudu_error!(ErrorCode::Internal, "repl ctl lock poisoned"))?
        .clone()
    {
        if ctl.is_primary() {
            return Ok(None);
        }
        return Ok(Some(
            ctl.primary_node_id()
                .filter(|primary| *primary != router.self_node_id()),
        ));
    }
    Ok(None)
}

/// Whether the snapshot manager's applied commit watermark has reached the
/// cluster stable epoch.
fn replica_caught_up(snapshot_mgr: &WorkerSnapshotMgr) -> bool {
    let Some(oracle) = snapshot_mgr.epoch_oracle() else {
        return false;
    };
    let stable = oracle.stable_epoch();
    // A replica with nothing applied yet (watermark 0) is caught up only
    // before the first commit exists.
    let applied = snapshot_mgr.applied_commit_epoch();
    applied >= stable
}

/// Migration fence for inbound partition RPCs: a migrating partition's
/// writes are rejected with `Migrating` (definitively not executed, the
/// coordinator retries until the window closes); a partition whose
/// placement moved away from this worker is rejected with `StaleRoute`
/// (the coordinator re-resolves). `None` lets the request proceed.
pub(crate) async fn migration_fence(
    cluster_router: &Option<Arc<dyn ClusterRouter>>,
    meta_mgr: &Arc<dyn MetaMgr>,
    worker_id: OID,
    request: &PartitionRpcRequest,
) -> RS<Option<PartitionRpcResponse>> {
    let Some(router) = cluster_router else {
        return Ok(None);
    };
    let Some(partition_id) = request.route_partition() else {
        return Ok(None);
    };
    if partition_id == DEFAULT_UNPARTITIONED_TABLE_PARTITION_ID {
        return Ok(None);
    }
    let migrating = request.migration_fenced();
    let checked = request.ownership_checked();
    if !migrating && !checked {
        return Ok(None);
    }
    let Some((rule, partition)) = meta_mgr.find_partition_names(partition_id).await? else {
        return Ok(None);
    };
    // The Migrating fence covers writes always and reads when the
    // re-homing kind requires it (splits/merges move rows between
    // partitions in place; a read mid-split could otherwise miss rows
    // still sitting in the parent's range).
    let blocked = match router.migration_state(&rule, &partition) {
        ::catalog::contract::cluster_router::MigrationState::Migrating { fence_reads, .. } => {
            request.migration_fenced()
                || (fence_reads
                    && matches!(
                        request,
                        PartitionRpcRequest::ReadKey { .. } | PartitionRpcRequest::ReadRange { .. }
                    ))
        }
        ::catalog::contract::cluster_router::MigrationState::None => false,
    };
    if blocked {
        return Ok(Some(PartitionRpcResponse::Migrating));
    }
    if !checked {
        return Ok(None);
    }
    // Ownership: the placement must resolve to this worker on this node.
    let Some((group_id, worker_index)) = router.placement(&rule, &partition) else {
        return Ok(None);
    };
    let mine = group_id == router.self_group_id()
        && ::catalog::contract::derive_oid::derive_worker_oid(
            router.self_node_id(),
            worker_index as usize,
        ) == worker_id;
    Ok((!mine).then_some(PartitionRpcResponse::StaleRoute))
}

/// Commit-time cutover fence of every staged partition (phase 3): a
/// staged write of a migrating partition aborts the commit; the client
/// retries the whole transaction against the flipped placement.
pub(crate) async fn ensure_staged_partitions_write_open(
    cluster_router: &Option<Arc<dyn ClusterRouter>>,
    meta_mgr: &Arc<dyn MetaMgr>,
    staged: &std::collections::BTreeMap<
        ::xengine::tx_mgr::PhysicalRelationId,
        std::collections::BTreeMap<Vec<u8>, Option<Vec<u8>>>,
    >,
) -> RS<()> {
    if cluster_router.is_none() {
        return Ok(());
    }
    for relation_id in staged.keys() {
        ensure_partition_write_open_nowait(cluster_router, meta_mgr, relation_id.partition_id)
            .await?;
    }
    Ok(())
}

/// In-flight guard of one fenced partition-RPC write (phase 3): registers
/// on entry (before the fence check) and deregisters on drop, so the
/// cutover's drain sees every decider.
pub(crate) struct MigrationInflightGuard {
    map: Arc<SMutex<std::collections::HashMap<OID, usize>>>,
    partition_id: OID,
}

impl Drop for MigrationInflightGuard {
    fn drop(&mut self) {
        if let Ok(mut map) = self.map.lock() {
            if let Some(count) = map.get_mut(&self.partition_id) {
                *count -= 1;
                if *count == 0 {
                    map.remove(&self.partition_id);
                }
            }
        }
    }
}

/// Registers an in-flight fenced write of `partition_id`; the returned
/// guard deregisters on drop.
pub(crate) fn migration_inflight_enter(
    migration_inflight: &Arc<SMutex<std::collections::HashMap<OID, usize>>>,
    partition_id: OID,
) -> MigrationInflightGuard {
    let map = migration_inflight.clone();
    if let Ok(mut locked) = map.lock() {
        *locked.entry(partition_id).or_insert(0) += 1;
    }
    MigrationInflightGuard { map, partition_id }
}

/// In-flight fenced writes of `partition_id` on this worker.
pub(crate) fn migration_inflight_count(
    migration_inflight: &Arc<SMutex<std::collections::HashMap<OID, usize>>>,
    partition_id: OID,
) -> usize {
    migration_inflight
        .lock()
        .ok()
        .and_then(|map| map.get(&partition_id).copied())
        .unwrap_or(0)
}

/// Installs the partition adoption hook (phase 3).
pub fn set_partition_adoption(
    partition_adoption: &SMutex<
        Option<Arc<dyn ::catalog::contract::partition_adoption::PartitionAdoption>>,
    >,
    adoption: Arc<dyn ::catalog::contract::partition_adoption::PartitionAdoption>,
) -> RS<()> {
    *partition_adoption
        .lock()
        .map_err(|_| mudu_error!(ErrorCode::Internal, "partition adoption lock poisoned"))? =
        Some(adoption);
    Ok(())
}

/// The installed partition adoption hook, if any.
pub(crate) fn partition_adoption(
    partition_adoption: &SMutex<
        Option<Arc<dyn ::catalog::contract::partition_adoption::PartitionAdoption>>,
    >,
) -> Option<Arc<dyn ::catalog::contract::partition_adoption::PartitionAdoption>> {
    partition_adoption.lock().ok()?.clone()
}
