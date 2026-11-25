//! Per-worker in-memory runtime state of the queue shards hosted on this
//! worker.
//!
//! Kept as a leaf module (external deps only) so
//! [`crate::contract::WorkerXContract`] can hold the queue registry without
//! pulling the queue business module into the module cycle.

use ::xengine::api::QueueShard;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_sys::sync::SMutex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Identifies one queue shard: the queue relation oid plus the oid of the
/// partition hosting the shard (`0` for an unpartitioned queue).
pub type QueueShardId = (OID, OID);

/// Registry key of one shard placement.
pub fn shard_key(shard: &QueueShard) -> QueueShardId {
    (shard.queue_oid, shard.partition_id)
}

const LEGACY_PREFIX: &[u8] = b"\0mudu.queue.state.v1/";

/// KV key of the legacy (pre-sentinel) watermark record of a queue.
pub(crate) fn legacy_state_key(oid: OID) -> Vec<u8> {
    [LEGACY_PREFIX, &oid.to_be_bytes()].concat()
}

/// The queue oid of a legacy watermark KV key, when it is one.
pub(crate) fn legacy_state_oid(key: &[u8]) -> Option<OID> {
    let tail = key.strip_prefix(LEGACY_PREFIX)?;
    Some(OID::from_be_bytes(tail.try_into().ok()?))
}

/// Rejects queue runtime operations when this worker does not host the
/// shard's partition.
pub(crate) fn ensure_local_shard_owner(shard: &QueueShard, worker_id: OID) -> RS<()> {
    if shard.worker_id != worker_id {
        return Err(mudu_error!(
            ErrorCode::NotImplemented,
            format!(
                "queue {} shard {} runs on worker {}; worker {} cannot serve it locally",
                shard.queue_oid, shard.shard_index, shard.worker_id, worker_id
            )
        ));
    }
    Ok(())
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

    /// Encoded payload bytes currently retained by the shard.
    pub fn bytes(&self) -> u64 {
        self.bytes
    }

    /// Rejected enqueue attempts since this process started.
    pub fn overflow_count(&self) -> u64 {
        self.overflow_count
    }
}

/// Point-in-time copy of one queue shard's runtime state for the
/// observability endpoint: the counters plus the number of parked
/// `RECEIVE ... WAIT` waiters.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueueRuntimeSnapshot {
    pub(crate) table_id: OID,
    pub(crate) partition_id: OID,
    pub(crate) next_seq: u64,
    pub(crate) acked_frontier: u64,
    pub(crate) depth: u64,
    pub(crate) bytes: u64,
    pub(crate) overflow_count: u64,
    pub(crate) waiters: u64,
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
