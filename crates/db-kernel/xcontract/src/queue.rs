//! Per-worker in-memory runtime state of the queue shards hosted on this
//! worker.
//!
//! A queue is a relation: unpartitioned (partition 0) by default, or sharded
//! by a payload column when created with
//! `PARTITION BY GLOBAL RULE ... REFERENCES (<col>)`. Shard *i* of a sharded
//! queue lives on partition *i* of the rule; each `__seq` embeds its shard
//! index in the high bits (see `catalog::contract::queue`), so every message
//! is routable from its sequence alone. The worker owning a shard's partition
//! keeps the per-shard counters the SQL executors and the observability
//! endpoint need.
//!
//! The durable watermarks are the shard's sentinel row inside the queue
//! relation; the counters here mirror committed state for activation and
//! observability — aborted changes never publish. Databases upgraded from the
//! worker-KV watermark format rebuild from their live relation rows on first
//! use.

use crate::lifecycle_cluster::resolve_partition_worker;
use crate::ops::_begin_tx;

use ::catalog::contract::cluster_router::ClusterRouter;
use ::catalog::contract::meta_mgr::MetaMgr;
use ::catalog::contract::queue::{
    queue_seq_datum, queue_seq_local, queue_seq_shard, queue_status_datum, QUEUE_SEQ_ATTR,
    QUEUE_STATUS_ATTR, QUEUE_STATUS_READY,
};
use std::sync::Arc;

use crate::contract::WorkerXContract;
use crate::queue_state::{publish_queue_state, read_legacy_state, read_shard_sentinel_committed};
use crate::queue_types::{
    ensure_local_shard_owner, shard_key, QueueRegistry, QueueRuntimeSnapshot,
};
use crate::queue_wait_registry::QueueWaitRegistry;
use ::xengine::api::{QueueShard, VecDatum};
use ::xengine::partition_router::{PartitionRouter, DEFAULT_UNPARTITIONED_TABLE_PARTITION_ID};
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use tracing::debug;

/// Per-shard runtime stats of this worker for the observability endpoint:
/// counters of every loaded shard plus its parked-waiter count.
pub fn queue_runtime_snapshot(
    queue_registry: &QueueRegistry,
    queue_wait_registry: &QueueWaitRegistry,
) -> RS<Vec<QueueRuntimeSnapshot>> {
    let waiter_counts = queue_wait_registry.waiter_counts()?;
    Ok(queue_registry
        .snapshot()?
        .into_iter()
        .map(|((table_id, partition_id), runtime)| QueueRuntimeSnapshot {
            table_id,
            partition_id,
            next_seq: runtime.next_seq(),
            acked_frontier: runtime.acked_frontier(),
            depth: runtime.depth(),
            bytes: runtime.bytes(),
            overflow_count: runtime.overflow_count(),
            waiters: waiter_counts
                .get(&(table_id, partition_id))
                .copied()
                .unwrap_or(0),
        })
        .collect())
}

/// Resolves the shard layout of a queue: one shard per partition of the
/// bound rule (in rule order), or the single shard 0 on partition 0 for
/// an unpartitioned queue. A partition without placement information
/// resolves to this worker (the `resolve_partition_worker` convention:
/// no placement means local).
pub(crate) async fn resolve_queue_shards(
    meta_mgr: &Arc<dyn MetaMgr>,
    worker_id: OID,
    cluster_router: &Option<Arc<dyn ClusterRouter>>,
    default_unpartitioned_worker_id: OID,
    queue_oid: OID,
) -> RS<Vec<QueueShard>> {
    let Some(binding) = meta_mgr.get_table_partition_binding(queue_oid).await? else {
        let worker_id = resolve_partition_worker(
            cluster_router,
            meta_mgr,
            default_unpartitioned_worker_id,
            DEFAULT_UNPARTITIONED_TABLE_PARTITION_ID,
        )
        .await?
        .unwrap_or(worker_id);
        return Ok(vec![QueueShard {
            queue_oid,
            shard_index: 0,
            partition_id: DEFAULT_UNPARTITIONED_TABLE_PARTITION_ID,
            worker_id,
        }]);
    };
    let rule = meta_mgr.get_partition_rule_by_id(binding.rule_id).await?;
    let mut shards = Vec::with_capacity(rule.partitions.len());
    for (index, partition) in rule.partitions.iter().enumerate() {
        // No placement information means local (the
        // `resolve_partition_worker` convention).
        let worker_id = resolve_partition_worker(
            cluster_router,
            meta_mgr,
            default_unpartitioned_worker_id,
            partition.partition_id,
        )
        .await?
        .unwrap_or(worker_id);
        shards.push(QueueShard {
            queue_oid,
            shard_index: index as u64,
            partition_id: partition.partition_id,
            worker_id,
        });
    }
    Ok(shards)
}

/// Resolves the shard of `queue_oid` hosted on `partition_id` and verifies
/// this worker owns it (the partition-RPC handlers' entry gate).
pub(crate) async fn resolve_local_queue_shard(
    meta_mgr: &Arc<dyn MetaMgr>,
    worker_id: OID,
    cluster_router: &Option<Arc<dyn ClusterRouter>>,
    default_unpartitioned_worker_id: OID,
    queue_oid: OID,
    partition_id: OID,
) -> RS<QueueShard> {
    let shard = resolve_queue_shards(
        meta_mgr,
        worker_id,
        cluster_router,
        default_unpartitioned_worker_id,
        queue_oid,
    )
    .await?
    .into_iter()
    .find(|shard| shard.partition_id == partition_id)
    .ok_or_else(|| {
        mudu_error!(
            ErrorCode::ObjectNotFound,
            format!("queue {queue_oid} has no shard on partition {partition_id}")
        )
    })?;
    ensure_local_shard_owner(&shard, worker_id)?;
    Ok(shard)
}

/// Routes one enqueue payload row to its shard: the queue's partition
/// column value decides through the bound range rule; an unpartitioned
/// queue resolves to its single shard.
pub(crate) async fn route_payload_shard(
    meta_mgr: &Arc<dyn MetaMgr>,
    worker_id: OID,
    cluster_router: &Option<Arc<dyn ClusterRouter>>,
    default_unpartitioned_worker_id: OID,
    partition_router: &PartitionRouter,
    queue_oid: OID,
    payload: &VecDatum,
) -> RS<QueueShard> {
    let shards = resolve_queue_shards(
        meta_mgr,
        worker_id,
        cluster_router,
        default_unpartitioned_worker_id,
        queue_oid,
    )
    .await?;
    if shards.len() == 1 {
        return Ok(shards[0]);
    }
    let desc = meta_mgr.get_table_by_id(queue_oid).await?;
    let partition_id = partition_router
        .route_exact_partition(queue_oid, desc.as_ref(), payload)
        .await?
        .ok_or_else(|| {
            mudu_error!(
                ErrorCode::InvalidState,
                format!("queue {queue_oid} payload did not route to any partition")
            )
        })?;
    shards
        .into_iter()
        .find(|shard| shard.partition_id == partition_id)
        .ok_or_else(|| {
            mudu_error!(
                ErrorCode::InvalidState,
                format!("queue {queue_oid} has no shard on partition {partition_id}")
            )
        })
}

/// Resolves the shard of `queue_oid` hosting the sequence `seq`.
pub(crate) async fn resolve_queue_shard_of_seq(
    meta_mgr: &Arc<dyn MetaMgr>,
    worker_id: OID,
    cluster_router: &Option<Arc<dyn ClusterRouter>>,
    default_unpartitioned_worker_id: OID,
    queue_oid: OID,
    seq: u64,
) -> RS<QueueShard> {
    let shard_index = queue_seq_shard(seq);
    let shards = resolve_queue_shards(
        meta_mgr,
        worker_id,
        cluster_router,
        default_unpartitioned_worker_id,
        queue_oid,
    )
    .await?;
    shards
        .into_iter()
        .find(|shard| shard.shard_index == shard_index)
        .ok_or_else(|| {
            mudu_error!(
                ErrorCode::ObjectNotFound,
                format!(
                    "queue {} has no shard {} (sequence {})",
                    queue_oid, shard_index, seq
                )
            )
        })
}

impl WorkerXContract {
    /// Rebuilds the runtime counters of one queue shard from its committed
    /// relation rows; registers fresh counters for a shard without live rows.
    /// Rows still marked `claimed` (their claiming transaction crashed between
    /// commit and ACK) are reset to `ready` for redelivery, keeping their
    /// receive count so poison detection survives restarts.
    ///
    /// Called lazily on the first shard access after a restart (by then the
    /// WAL tail has been replayed into the relation; the server bootstrap runs
    /// before log recovery, so an eager bootstrap scan could miss committed
    /// rows).
    pub(crate) async fn rebuild_queue_shard_runtime_async(&self, shard: QueueShard) -> RS<()> {
        ensure_local_shard_owner(&shard, self.worker_id)?;
        let sentinel = read_shard_sentinel_committed(
            &self.meta_mgr,
            &self.storage,
            &self.snapshot_mgr,
            &self.tx_lock,
            self.worker_id,
            &shard,
        )
        .await?;
        let scan = self
            .storage
            .queue_rebuild_scan_async(shard.queue_oid, shard.partition_id)
            .await?;
        match (&sentinel, &scan) {
            (Some(state), _) => {
                publish_queue_state(
                    &self.queue_registry,
                    shard_key(&shard),
                    state.next,
                    state.frontier,
                    state.depth,
                    state.bytes,
                )?;
            }
            (None, Some(scan)) => {
                self.queue_registry.rebuild(
                    shard_key(&shard),
                    queue_seq_local(scan.min_seq),
                    queue_seq_local(scan.max_seq),
                )?;
            }
            (None, None) => {
                // A pre-sentinel database may still hold the watermark in the
                // legacy worker-KV record (unpartitioned queues only).
                let legacy = if shard.partition_id == DEFAULT_UNPARTITIONED_TABLE_PARTITION_ID {
                    read_legacy_state(&self.storage, shard.queue_oid).await?
                } else {
                    None
                };
                match legacy {
                    Some(state) => {
                        publish_queue_state(
                            &self.queue_registry,
                            shard_key(&shard),
                            state.next,
                            state.frontier,
                            state.depth,
                            state.bytes,
                        )?;
                    }
                    None => {
                        self.queue_registry.register(shard_key(&shard))?;
                    }
                }
            }
        }
        if let Some(scan) = scan {
            if !scan.claimed_seqs.is_empty() {
                self.reset_claimed_rows_async(&shard, &scan.claimed_seqs)
                    .await?;
            }
        }
        Ok(())
    }

    /// Re-delivers crashed claims of one shard: rewrite each row's status
    /// back to `ready` in one ordinary transaction (WAL-logged, so replicas
    /// converge through replay). On a non-serving replica the commit is
    /// rejected; the serving leader's own reset replicates, so the rejection
    /// is safe to ignore.
    async fn reset_claimed_rows_async(&self, shard: &QueueShard, seqs: &[u64]) -> RS<()> {
        let desc = self.meta_mgr.get_table_by_id(shard.queue_oid).await?;
        let tx_mgr = _begin_tx(&self.snapshot_mgr)?;
        for seq in seqs {
            self.update_pinned(
                desc.clone(),
                tx_mgr.clone(),
                shard.queue_oid,
                Some(shard.partition_id),
                &VecDatum::new(vec![(QUEUE_SEQ_ATTR, queue_seq_datum(*seq)?)]),
                &VecDatum::new(vec![(
                    QUEUE_STATUS_ATTR,
                    queue_status_datum(QUEUE_STATUS_READY)?,
                )]),
                &[],
                &[],
            )
            .await?;
        }
        match self.worker_commit_routed_tx_async(tx_mgr).await {
            Ok(_) => Ok(()),
            Err(err) if err.ec() == ErrorCode::DomainViolation => {
                debug!(
                    queue_oid = shard.queue_oid,
                    partition_id = shard.partition_id,
                    "queue claim reset skipped: this worker is not the serving leader"
                );
                Ok(())
            }
            Err(err) => Err(err),
        }
    }

    /// Loads the runtime counters of `shard` from storage when they are not
    /// in memory yet.
    pub(crate) async fn ensure_shard_runtime_loaded(&self, shard: &QueueShard) -> RS<()> {
        if self.queue_registry.contains(shard_key(shard))? {
            return Ok(());
        }
        self.rebuild_queue_shard_runtime_async(*shard).await
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn alloc_advances_and_checks_depth_cap() {
        let registry = QueueRegistry::new();
        let first = registry.alloc((7, 0), 3, 10).unwrap();
        assert_eq!(first, 0);
        let next = registry.alloc((7, 0), 2, 10).unwrap();
        assert_eq!(next, 3);
        let runtime = registry.get((7, 0)).unwrap().unwrap();
        assert_eq!(runtime.next_seq(), 5);
        assert_eq!(runtime.depth(), 5);
    }

    #[test]
    fn shards_of_one_queue_are_independent() {
        let registry = QueueRegistry::new();
        registry.alloc((7, 100), 3, 10).unwrap();
        registry.alloc((7, 200), 1, 10).unwrap();
        assert_eq!(registry.get((7, 100)).unwrap().unwrap().next_seq(), 3);
        assert_eq!(registry.get((7, 200)).unwrap().unwrap().next_seq(), 1);
        registry.unregister_queue(7).unwrap();
        assert!(registry.get((7, 100)).unwrap().is_none());
        assert!(registry.get((7, 200)).unwrap().is_none());
    }

    #[test]
    fn alloc_fails_with_queue_full_at_the_cap() {
        let registry = QueueRegistry::new();
        registry.alloc((7, 0), 10, 10).unwrap();
        let err = registry.alloc((7, 0), 1, 10).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::QueueFull);
    }

    #[test]
    fn frontier_advance_relaxes_the_depth_cap() {
        let registry = QueueRegistry::new();
        registry.alloc((7, 0), 10, 10).unwrap();
        registry.advance_acked_frontier((7, 0), 5).unwrap();
        let runtime = registry.get((7, 0)).unwrap().unwrap();
        assert_eq!(runtime.acked_frontier(), 6);
        assert_eq!(runtime.depth(), 4);
        registry.alloc((7, 0), 6, 10).unwrap();
        assert_eq!(registry.get((7, 0)).unwrap().unwrap().next_seq(), 16);
    }

    #[test]
    fn frontier_advance_is_monotonic_and_clamped() {
        let registry = QueueRegistry::new();
        registry.alloc((7, 0), 4, 100).unwrap();
        registry.advance_acked_frontier((7, 0), 2).unwrap();
        // Moving backwards is a no-op.
        registry.advance_acked_frontier((7, 0), 1).unwrap();
        assert_eq!(registry.get((7, 0)).unwrap().unwrap().acked_frontier(), 3);
        // An ACK past the allocation watermark clamps at it.
        registry
            .advance_acked_frontier((7, 0), u64::MAX - 1)
            .unwrap();
        let runtime = registry.get((7, 0)).unwrap().unwrap();
        assert_eq!(runtime.acked_frontier(), 4);
        assert_eq!(runtime.depth(), 0);
    }

    #[test]
    fn rebuild_restores_counters_from_relation_bounds() {
        let registry = QueueRegistry::new();
        registry.rebuild((7, 0), 40, 42).unwrap();
        let runtime = registry.get((7, 0)).unwrap().unwrap();
        assert_eq!(runtime.next_seq(), 43);
        assert_eq!(runtime.acked_frontier(), 40);
        assert_eq!(runtime.depth(), 3);
        let first = registry.alloc((7, 0), 1, 10).unwrap();
        assert_eq!(first, 43);
    }

    #[test]
    fn unregister_drops_the_counters() {
        let registry = QueueRegistry::new();
        registry.register((7, 0)).unwrap();
        registry.alloc((7, 0), 5, 10).unwrap();
        registry.unregister((7, 0)).unwrap();
        assert!(registry.get((7, 0)).unwrap().is_none());
    }

    #[test]
    fn alloc_zero_count_is_rejected() {
        let registry = QueueRegistry::new();
        let err = registry.alloc((7, 0), 0, 10).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::InvalidArgument);
    }
}
