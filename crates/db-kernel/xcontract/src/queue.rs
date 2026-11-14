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

use std::collections::HashMap;

use ::catalog::contract::queue::{
    queue_seq_datum, queue_seq_local, queue_seq_shard, queue_status_datum, QUEUE_SEQ_ATTR,
    QUEUE_STATUS_ATTR, QUEUE_STATUS_READY,
};
use serde::{Deserialize, Serialize};

use super::*;

/// Identifies one queue shard: the queue relation oid plus the oid of the
/// partition hosting the shard (`0` for an unpartitioned queue).
pub type QueueShardId = (OID, OID);

/// Registry key of one shard placement.
pub fn shard_key(shard: &QueueShard) -> QueueShardId {
    (shard.queue_oid, shard.partition_id)
}

/// Runtime counters of one queue shard.
///
/// `acked_frontier` is in "every local `__seq` below this value is done"
/// units, so a fresh shard starts at `(next_seq = 0, acked_frontier = 0)`
/// with depth 0. All sequences here are per-shard local values (without the
/// shard bits).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct QueueRuntime {
    next_seq: u64,
    acked_frontier: u64,
    depth: u64,
    bytes: u64,
    overflow_count: u64,
}

impl QueueRuntime {
    /// The next local sequence number to allocate.
    pub fn next_seq(&self) -> u64 {
        self.next_seq
    }

    /// The acked frontier: every local `__seq` below it is acknowledged.
    pub fn acked_frontier(&self) -> u64 {
        self.acked_frontier
    }

    /// The number of committed message rows retained by the shard.
    pub fn depth(&self) -> u64 {
        self.depth
    }
}

/// Point-in-time copy of one queue shard's runtime state for the
/// observability endpoint: the counters plus the number of parked
/// `RECEIVE ... WAIT` waiters.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueueRuntimeSnapshot {
    table_id: OID,
    partition_id: OID,
    next_seq: u64,
    acked_frontier: u64,
    depth: u64,
    bytes: u64,
    overflow_count: u64,
    waiters: u64,
}

impl QueueRuntimeSnapshot {
    /// The queue's table OID.
    pub fn table_id(&self) -> OID {
        self.table_id
    }

    /// The partition hosting this shard (`0` for an unpartitioned queue).
    pub fn partition_id(&self) -> OID {
        self.partition_id
    }

    /// The next local sequence number to allocate.
    pub fn next_seq(&self) -> u64 {
        self.next_seq
    }

    /// The acked frontier: every local `__seq` below it is acknowledged.
    pub fn acked_frontier(&self) -> u64 {
        self.acked_frontier
    }

    /// The number of committed message rows retained by the shard.
    pub fn depth(&self) -> u64 {
        self.depth
    }

    /// Encoded payload bytes currently retained by the shard.
    pub fn bytes(&self) -> u64 {
        self.bytes
    }

    /// Rejected enqueue attempts since this process started.
    pub fn overflow_count(&self) -> u64 {
        self.overflow_count
    }

    /// The number of parked `RECEIVE ... WAIT` waiters.
    pub fn waiters(&self) -> u64 {
        self.waiters
    }
}

/// Per-worker registry of queue-shard runtime counters, keyed by
/// [`QueueShardId`].
pub struct QueueRegistry {
    inner: SMutex<HashMap<QueueShardId, QueueRuntime>>,
    limits: SMutex<(u64, u64)>,
    /// Round-robin cursor of keyless `RECEIVE` statements: queue oid → next
    /// shard index to drain.
    receive_cursor: SMutex<HashMap<OID, u64>>,
}

impl QueueRegistry {
    /// Creates an empty registry.
    pub fn new() -> Self {
        Self {
            inner: SMutex::new(HashMap::new()),
            limits: SMutex::new((65536, 64 * 1024 * 1024)),
            receive_cursor: SMutex::new(HashMap::new()),
        }
    }

    /// The next shard index a keyless `RECEIVE` of the queue drains
    /// (rotating over `shard_count` shards).
    pub fn rotate_receive_shard(&self, queue_oid: OID, shard_count: u64) -> RS<u64> {
        let mut guard = self.receive_cursor.lock()?;
        let cursor = guard.entry(queue_oid).or_insert(0);
        let shard = *cursor % shard_count.max(1);
        *cursor = cursor.wrapping_add(1);
        Ok(shard)
    }

    fn lock(&self) -> RS<mudu_sys::sync::SMutexGuard<'_, HashMap<QueueShardId, QueueRuntime>>> {
        self.inner.lock()
    }

    /// Registers a freshly created queue shard; a pre-existing entry is kept.
    pub fn register(&self, shard: QueueShardId) -> RS<()> {
        self.lock()?.entry(shard).or_default();
        Ok(())
    }

    pub(crate) fn set_durable(
        &self,
        shard: QueueShardId,
        next_seq: u64,
        frontier: u64,
        depth: u64,
        bytes: u64,
    ) -> RS<()> {
        let mut entries = self.lock()?;
        let overflow_count = entries.get(&shard).map_or(0, |entry| entry.overflow_count);
        entries.insert(
            shard,
            QueueRuntime {
                next_seq,
                acked_frontier: frontier,
                depth,
                bytes,
                overflow_count,
            },
        );
        Ok(())
    }

    /// Sets per-shard message and encoded-payload byte limits.
    pub fn configure_limits(&self, messages: u64, bytes: u64) -> RS<()> {
        if messages == 0 || bytes == 0 {
            return Err(mudu_error!(
                ErrorCode::InvalidArgument,
                "queue limits must be positive"
            ));
        }
        *self.limits.lock()? = (messages, bytes);
        Ok(())
    }
    pub(crate) fn limits(&self) -> RS<(u64, u64)> {
        Ok(*self.limits.lock()?)
    }
    pub(crate) fn record_overflow(&self, shard: QueueShardId) -> RS<()> {
        let mut entries = self.lock()?;
        let state = entries.entry(shard).or_default();
        state.overflow_count = state.overflow_count.saturating_add(1);
        Ok(())
    }

    /// Drops the runtime state of one shard.
    pub fn unregister(&self, shard: QueueShardId) -> RS<()> {
        self.lock()?.remove(&shard);
        Ok(())
    }

    /// Drops the runtime state of every shard of a dropped queue.
    pub fn unregister_queue(&self, table_id: OID) -> RS<()> {
        self.lock()?.retain(|(oid, _), _| *oid != table_id);
        Ok(())
    }

    /// Allocates `count` consecutive local sequence numbers for `shard`,
    /// returning the first of them. Fails with [`ErrorCode::QueueFull`] when
    /// the allocation would push the shard depth past `max_depth`. Unknown
    /// shards start empty (the create path registers explicitly; the lazy
    /// rebuild covers restarts).
    pub fn alloc(&self, shard: QueueShardId, count: u64, max_depth: u64) -> RS<u64> {
        if count == 0 {
            return Err(mudu_error!(
                ErrorCode::InvalidArgument,
                "queue sequence allocation of zero sequences"
            ));
        }
        let mut guard = self.lock()?;
        let runtime = guard.entry(shard).or_default();
        if runtime.depth() + count > max_depth {
            return Err(mudu_error!(
                ErrorCode::QueueFull,
                format!(
                    "queue {} is full: depth {} + {} exceeds the cap {}",
                    shard.0,
                    runtime.depth(),
                    count,
                    max_depth
                )
            ));
        }
        let first = runtime.next_seq;
        runtime.next_seq += count;
        runtime.depth += count;
        Ok(first)
    }

    /// Advances the acked frontier of `shard` past local `seq` (clamped at
    /// the allocation watermark so a mis-targeted ACK can never push the
    /// depth arithmetic negative).
    pub fn advance_acked_frontier(&self, shard: QueueShardId, seq: u64) -> RS<()> {
        let mut guard = self.lock()?;
        let runtime = guard.entry(shard).or_default();
        let previous = runtime.acked_frontier;
        runtime.acked_frontier = runtime
            .acked_frontier
            .max(seq.saturating_add(1))
            .min(runtime.next_seq);
        runtime.depth = runtime
            .depth
            .saturating_sub(runtime.acked_frontier - previous);
        Ok(())
    }

    /// Rebuilds the counters of `shard` from the committed relation
    /// head/tail after a restart: `min_seq`/`max_seq` are the smallest and
    /// largest live local `__seq` values; everything below `min_seq` was
    /// acknowledged and physically deleted.
    pub fn rebuild(&self, shard: QueueShardId, min_seq: u64, max_seq: u64) -> RS<()> {
        let mut guard = self.lock()?;
        guard.insert(
            shard,
            QueueRuntime {
                next_seq: max_seq + 1,
                acked_frontier: min_seq,
                depth: max_seq - min_seq + 1,
                bytes: 0,
                overflow_count: 0,
            },
        );
        Ok(())
    }

    /// Reads the counters of `shard`, when loaded.
    pub fn get(&self, shard: QueueShardId) -> RS<Option<QueueRuntime>> {
        Ok(self.lock()?.get(&shard).copied())
    }

    /// Point-in-time copy of every registered shard's counters.
    pub fn snapshot(&self) -> RS<Vec<(QueueShardId, QueueRuntime)>> {
        Ok(self
            .lock()?
            .iter()
            .map(|(id, runtime)| (*id, *runtime))
            .collect())
    }

    /// Whether `shard` has runtime counters loaded.
    pub fn contains(&self, shard: QueueShardId) -> RS<bool> {
        Ok(self.lock()?.contains_key(&shard))
    }

    /// Drops every entry, forcing a lazy rebuild from the relations (used
    /// when storage is reloaded underneath, e.g. by a snapshot restore).
    pub fn clear(&self) -> RS<()> {
        self.lock()?.clear();
        Ok(())
    }
}

impl Default for QueueRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl WorkerXContract {
    /// Resolves the shard layout of a queue: one shard per partition of the
    /// bound rule (in rule order), or the single shard 0 on partition 0 for
    /// an unpartitioned queue. A partition without placement information
    /// resolves to this worker (the `resolve_partition_worker` convention:
    /// no placement means local).
    pub(crate) async fn resolve_queue_shards(&self, queue_oid: OID) -> RS<Vec<QueueShard>> {
        let Some(binding) = self.meta_mgr.get_table_partition_binding(queue_oid).await? else {
            let worker_id = self
                .resolve_partition_worker(DEFAULT_UNPARTITIONED_TABLE_PARTITION_ID)
                .await?
                .unwrap_or(self.worker_id);
            return Ok(vec![QueueShard {
                queue_oid,
                shard_index: 0,
                partition_id: DEFAULT_UNPARTITIONED_TABLE_PARTITION_ID,
                worker_id,
            }]);
        };
        let rule = self
            .meta_mgr
            .get_partition_rule_by_id(binding.rule_id)
            .await?;
        let mut shards = Vec::with_capacity(rule.partitions.len());
        for (index, partition) in rule.partitions.iter().enumerate() {
            // No placement information means local (the
            // `resolve_partition_worker` convention).
            let worker_id = self
                .resolve_partition_worker(partition.partition_id)
                .await?
                .unwrap_or(self.worker_id);
            shards.push(QueueShard {
                queue_oid,
                shard_index: index as u64,
                partition_id: partition.partition_id,
                worker_id,
            });
        }
        Ok(shards)
    }

    /// Routes one enqueue payload row to its shard: the queue's partition
    /// column value decides through the bound range rule; an unpartitioned
    /// queue resolves to its single shard.
    pub(crate) async fn route_payload_shard(
        &self,
        queue_oid: OID,
        payload: &VecDatum,
    ) -> RS<QueueShard> {
        let shards = self.resolve_queue_shards(queue_oid).await?;
        if shards.len() == 1 {
            return Ok(shards[0]);
        }
        let desc = self.meta_mgr.get_table_by_id(queue_oid).await?;
        let partition_id = self
            .partition_router
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
        &self,
        queue_oid: OID,
        seq: u64,
    ) -> RS<QueueShard> {
        let shard_index = queue_seq_shard(seq);
        let shards = self.queue_shards(queue_oid).await?;
        shards
            .into_iter()
            .find(|shard| shard.shard_index == shard_index)
            .ok_or_else(|| {
                mudu_error!(
                    ErrorCode::EntityNotFound,
                    format!(
                        "queue {} has no shard {} (sequence {})",
                        queue_oid, shard_index, seq
                    )
                )
            })
    }

    /// Resolves the shard of `queue_oid` hosted on `partition_id` and
    /// verifies this worker owns it (the partition-RPC handlers' entry gate).
    pub(crate) async fn resolve_local_queue_shard(
        &self,
        queue_oid: OID,
        partition_id: OID,
    ) -> RS<QueueShard> {
        let shard = self
            .resolve_queue_shards(queue_oid)
            .await?
            .into_iter()
            .find(|shard| shard.partition_id == partition_id)
            .ok_or_else(|| {
                mudu_error!(
                    ErrorCode::EntityNotFound,
                    format!("queue {queue_oid} has no shard on partition {partition_id}")
                )
            })?;
        self.ensure_local_shard_owner(&shard)?;
        Ok(shard)
    }

    /// Rejects queue runtime operations when this worker does not host the
    /// shard's partition.
    pub(crate) fn ensure_local_shard_owner(&self, shard: &QueueShard) -> RS<()> {
        if shard.worker_id != self.worker_id {
            return Err(mudu_error!(
                ErrorCode::NotImplemented,
                format!(
                    "queue {} shard {} runs on worker {}; worker {} cannot serve it locally",
                    shard.queue_oid, shard.shard_index, shard.worker_id, self.worker_id
                )
            ));
        }
        Ok(())
    }

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
        self.ensure_local_shard_owner(&shard)?;
        let sentinel = self.read_shard_sentinel_committed(&shard).await?;
        let scan = self
            .storage
            .queue_rebuild_scan_async(shard.queue_oid, shard.partition_id)
            .await?;
        match (&sentinel, &scan) {
            (Some(state), _) => {
                self.publish_queue_state(
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
                    self.read_legacy_state(shard.queue_oid).await?
                } else {
                    None
                };
                match legacy {
                    Some(state) => {
                        self.publish_queue_state(
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
        let tx_mgr = self._begin_tx()?;
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
            Ok(()) => Ok(()),
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

    /// Per-shard runtime stats of this worker for the observability endpoint:
    /// counters of every loaded shard plus its parked-waiter count.
    pub fn queue_runtime_snapshot(&self) -> RS<Vec<QueueRuntimeSnapshot>> {
        let waiter_counts = self.queue_wait_registry.waiter_counts()?;
        Ok(self
            .queue_registry
            .snapshot()?
            .into_iter()
            .map(|((table_id, partition_id), runtime)| QueueRuntimeSnapshot {
                table_id,
                partition_id,
                next_seq: runtime.next_seq(),
                acked_frontier: runtime.acked_frontier(),
                depth: runtime.depth,
                bytes: runtime.bytes,
                overflow_count: runtime.overflow_count,
                waiters: waiter_counts
                    .get(&(table_id, partition_id))
                    .copied()
                    .unwrap_or(0),
            })
            .collect())
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
