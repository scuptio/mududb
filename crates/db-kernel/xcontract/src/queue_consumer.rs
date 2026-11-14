//! Named consumer cursors of queue shards (`CREATE CONSUMER cg ON q`).
//!
//! Durable cursor state lives in the `_queue_consumer` system table (one row
//! per `(queue_oid, shard, consumer)` holding the per-shard cursor frontier);
//! the in-memory [`ConsumerRegistry`] mirrors it per worker.
//!
//! Frontier units are "every local `__seq` below the frontier is acknowledged
//! by this cursor on this shard" — the same units as
//! [`crate::QueueRuntime::acked_frontier`]. A fresh cursor starts at 0 and
//! therefore sees the whole shard including local `__seq` 0.
//! `ACK q FOR cg TO seq` moves the frontier of the shard embedded in `seq` to
//! `max(frontier, local(seq) + 1)`. The fan-out GC physically deletes shard
//! rows below `min(frontier)` over all of the shard's cursors — no consumer
//! will ever scan them again.
use super::*;
use ::catalog::contract::queue::{
    decode_queue_seq, queue_seq_datum, queue_seq_encode, queue_seq_local, QUEUE_PAYLOAD_ATTR_START,
    QUEUE_SEQ_ATTR, QUEUE_SEQ_LOCAL_SENTINEL,
};
use ::meta::queue_consumer::{
    decode_queue_consumer_frontier, decode_queue_consumer_name, decode_queue_consumer_oid,
    decode_queue_consumer_shard, queue_consumer_frontier_datum, queue_consumer_name_datum,
    queue_consumer_oid_datum, queue_consumer_shard_datum, QUEUE_CONSUMER_FRONTIER_ATTR,
    QUEUE_CONSUMER_NAME_ATTR, QUEUE_CONSUMER_OID_ATTR, QUEUE_CONSUMER_SHARD_ATTR,
    QUEUE_CONSUMER_TABLE_ID,
};
use std::collections::HashMap;

/// `(queue_oid, shard)` → (`consumer` → local frontier).
type CursorMap = HashMap<(OID, u64), HashMap<String, u64>>;

/// Per-worker cursor cache of queue-shard consumers.
pub struct ConsumerRegistry {
    inner: SMutex<CursorMap>,
}

impl ConsumerRegistry {
    /// Creates an empty registry.
    pub fn new() -> Self {
        Self {
            inner: SMutex::new(HashMap::new()),
        }
    }

    fn lock(&self) -> RS<mudu_sys::sync::SMutexGuard<'_, CursorMap>> {
        self.inner.lock()
    }

    /// Whether the cursors of the queue shard are loaded in memory.
    pub fn is_loaded(&self, queue_oid: OID, shard: u64) -> RS<bool> {
        Ok(self.lock()?.contains_key(&(queue_oid, shard)))
    }

    /// Replaces the cache entries of the queue shard (the post-restart
    /// rebuild from the committed `_queue_consumer` rows).
    pub fn load(&self, queue_oid: OID, shard: u64, entries: Vec<(String, u64)>) -> RS<()> {
        self.lock()?
            .insert((queue_oid, shard), entries.into_iter().collect());
        Ok(())
    }

    /// Registers a fresh cursor at frontier 0; a pre-existing entry (a
    /// phantom left by an aborted create) is reset.
    pub fn register(&self, queue_oid: OID, shard: u64, name: &str) -> RS<()> {
        self.lock()?
            .entry((queue_oid, shard))
            .or_default()
            .insert(name.to_string(), 0);
        Ok(())
    }

    /// Removes a cursor.
    pub fn unregister(&self, queue_oid: OID, shard: u64, name: &str) -> RS<()> {
        if let Some(cursors) = self.lock()?.get_mut(&(queue_oid, shard)) {
            cursors.remove(name);
        }
        Ok(())
    }

    /// Drops every entry of the queue (all shards).
    pub fn unregister_queue(&self, queue_oid: OID) -> RS<()> {
        self.lock()?.retain(|(oid, _), _| *oid != queue_oid);
        Ok(())
    }

    /// Reads the frontier of the cursor `name` on the queue shard; `None`
    /// when the cursor is unknown.
    pub fn frontier(&self, queue_oid: OID, shard: u64, name: &str) -> RS<Option<u64>> {
        Ok(self
            .lock()?
            .get(&(queue_oid, shard))
            .and_then(|cursors| cursors.get(name).copied()))
    }

    /// Advances the frontier of the cursor `name` on the queue shard to
    /// `max(current, frontier)` under the registry lock and returns the merged
    /// frontier, so concurrent acks of one cursor can never push the stored
    /// frontier backwards. `None` when the cursor is unknown.
    pub fn advance_frontier(
        &self,
        queue_oid: OID,
        shard: u64,
        name: &str,
        frontier: u64,
    ) -> RS<Option<u64>> {
        let mut guard = self.lock()?;
        let Some(cursor) = guard
            .get_mut(&(queue_oid, shard))
            .and_then(|cursors| cursors.get_mut(name))
        else {
            return Ok(None);
        };
        *cursor = (*cursor).max(frontier);
        Ok(Some(*cursor))
    }

    /// The smallest frontier over all cursors of the queue shard (the fan-out
    /// GC frontier); `None` when the shard has no cursors.
    pub fn min_frontier(&self, queue_oid: OID, shard: u64) -> RS<Option<u64>> {
        Ok(self
            .lock()?
            .get(&(queue_oid, shard))
            .and_then(|cursors| cursors.values().copied().min()))
    }

    /// The cursors of the queue shard as `(name, frontier)` pairs sorted by
    /// name.
    pub fn list(&self, queue_oid: OID, shard: u64) -> RS<Vec<(String, u64)>> {
        let mut entries: Vec<(String, u64)> = self
            .lock()?
            .get(&(queue_oid, shard))
            .map(|cursors| {
                cursors
                    .iter()
                    .map(|(name, frontier)| (name.clone(), *frontier))
                    .collect()
            })
            .unwrap_or_default();
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(entries)
    }

    /// Whether the queue shard has at least one cursor.
    pub fn exists(&self, queue_oid: OID, shard: u64) -> RS<bool> {
        Ok(self
            .lock()?
            .get(&(queue_oid, shard))
            .map(|cursors| !cursors.is_empty())
            .unwrap_or(false))
    }

    /// Whether any shard of the queue has at least one cursor.
    pub fn exists_queue(&self, queue_oid: OID) -> RS<bool> {
        Ok(self
            .lock()?
            .iter()
            .any(|((oid, _), cursors)| *oid == queue_oid && !cursors.is_empty()))
    }

    /// Drops every entry, forcing a lazy rebuild from `_queue_consumer` (used
    /// when storage is reloaded underneath, e.g. by a snapshot restore).
    pub fn clear(&self) -> RS<()> {
        self.lock()?.clear();
        Ok(())
    }
}

impl Default for ConsumerRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Builds the `_queue_consumer` key datum of the cursor `consumer` on the
/// queue shard.
fn consumer_key(queue_oid: OID, shard: u64, consumer: &str) -> RS<VecDatum> {
    Ok(VecDatum::new(vec![
        (QUEUE_CONSUMER_OID_ATTR, queue_consumer_oid_datum(queue_oid)),
        (
            QUEUE_CONSUMER_SHARD_ATTR,
            queue_consumer_shard_datum(shard)?,
        ),
        (
            QUEUE_CONSUMER_NAME_ATTR,
            queue_consumer_name_datum(consumer)?,
        ),
    ]))
}

/// Decodes one `_queue_consumer` row into its `(queue_oid, shard, consumer,
/// frontier)` parts; `None` when any field is missing.
fn decode_consumer_row(fields: &[Option<Buf>]) -> RS<Option<(OID, u64, String, u64)>> {
    if let [Some(oid), Some(shard), Some(name), Some(frontier)] = fields {
        Ok(Some((
            decode_queue_consumer_oid(oid)?,
            decode_queue_consumer_shard(shard)?,
            decode_queue_consumer_name(name)?,
            decode_queue_consumer_frontier(frontier)?,
        )))
    } else {
        Ok(None)
    }
}

impl WorkerXContract {
    /// The `(queue_oid, shard_index)` pairs whose consumer cursor rows the
    /// transaction stages (create/drop/ack of consumers); empty when the
    /// write set holds no `_queue_consumer` rows.
    pub(crate) fn staged_consumer_shards(&self, tx: &dyn TxMgr) -> RS<Vec<(OID, u64)>> {
        let staged = tx.staged_relation_ops();
        let Some(rows) = staged.get(&PhysicalRelationId {
            table_id: QUEUE_CONSUMER_TABLE_ID,
            partition_id: DEFAULT_UNPARTITIONED_TABLE_PARTITION_ID,
        }) else {
            return Ok(Vec::new());
        };
        if rows.is_empty() {
            return Ok(Vec::new());
        }
        let desc = ::meta::queue_consumer::queue_consumer_desc()?;
        // Tuple field order is canonical (not attribute order): map each key
        // attribute through its datum index.
        let oid_field = desc
            .key_desc()
            .get_field_desc(desc.get_attr(QUEUE_CONSUMER_OID_ATTR).datum_index());
        let shard_field = desc
            .key_desc()
            .get_field_desc(desc.get_attr(QUEUE_CONSUMER_SHARD_ATTR).datum_index());
        let mut out: Vec<(OID, u64)> = Vec::new();
        for key in rows.keys() {
            let oid = decode_queue_consumer_oid(oid_field.get(key)?)?;
            let shard = decode_queue_consumer_shard(shard_field.get(key)?)?;
            out.push((oid, shard));
        }
        out.sort_unstable();
        out.dedup();
        Ok(out)
    }

    /// Reads committed cursor rows plus this transaction's staged changes.
    /// The cursor table is unpartitioned; the pinned read reaches its owner
    /// worker from anywhere.
    pub(crate) async fn consumer_scan_in_tx(
        &self,
        tx: Arc<dyn TxMgr>,
        queue_oid: OID,
    ) -> RS<Vec<QueueConsumerEntry>> {
        let cursor = self
            .read_range_on_partition(
                tx,
                QUEUE_CONSUMER_TABLE_ID,
                DEFAULT_UNPARTITIONED_TABLE_PARTITION_ID,
                &RangeData::new(Bound::Unbounded, Bound::Unbounded),
                &VecSelTerm::new(vec![
                    QUEUE_CONSUMER_OID_ATTR,
                    QUEUE_CONSUMER_SHARD_ATTR,
                    QUEUE_CONSUMER_NAME_ATTR,
                    QUEUE_CONSUMER_FRONTIER_ATTR,
                ]),
                &OptRead::default(),
            )
            .await?;
        let mut entries = Vec::new();
        while let Some(row) = cursor.next().await? {
            let fields = row.into_fields();
            if let Some((oid, shard, name, frontier)) = decode_consumer_row(&fields)? {
                if oid == queue_oid {
                    entries.push(QueueConsumerEntry {
                        shard,
                        name,
                        frontier,
                    });
                }
            }
        }
        entries.sort_by(|a, b| a.name.cmp(&b.name).then(a.shard.cmp(&b.shard)));
        Ok(entries)
    }

    /// Force-refreshes the cursor cache of the queue shard from the committed
    /// `_queue_consumer` rows (the post-commit consumer-change path).
    pub(crate) async fn reload_consumer_state(&self, queue_oid: OID, shard_index: u64) -> RS<()> {
        let tx = self._begin_tx()?;
        let entries = self.consumer_scan_in_tx(tx.clone(), queue_oid).await;
        let _ = self.worker_rollback_tx(tx);
        let shard_entries = entries?
            .into_iter()
            .filter(|entry| entry.shard == shard_index)
            .map(|entry| (entry.name, entry.frontier))
            .collect();
        self.consumer_registry
            .load(queue_oid, shard_index, shard_entries)
    }

    /// Loads the cursor state of the queue shard from the committed
    /// `_queue_consumer` rows when it is not in memory yet (once per shard
    /// per process, mirroring the queue runtime counters' lazy rebuild).
    pub(crate) async fn ensure_consumer_state_loaded(
        &self,
        queue_oid: OID,
        shard_index: u64,
    ) -> RS<()> {
        if self.consumer_registry.is_loaded(queue_oid, shard_index)? {
            return Ok(());
        }
        let tx = self._begin_tx()?;
        let entries = self.consumer_scan_in_tx(tx.clone(), queue_oid).await;
        let _ = self.worker_rollback_tx(tx);
        let entries = entries?;
        let shard_entries = entries
            .into_iter()
            .filter(|entry| entry.shard == shard_index)
            .map(|entry| (entry.name, entry.frontier))
            .collect();
        self.consumer_registry
            .load(queue_oid, shard_index, shard_entries)
    }

    /// `CREATE CONSUMER`: inserts one cursor row (frontier 0) per shard of the
    /// queue in the caller's transaction; the committed cache refresh happens
    /// after commit. The read-then-insert pair reports a duplicate cursor as
    /// `EntityAlreadyExists`; the insert itself is the authoritative guard
    /// against a concurrently committing create.
    pub(crate) async fn consumer_create_async(
        &self,
        tx_mgr: Arc<dyn TxMgr>,
        queue_oid: OID,
        consumer: &str,
    ) -> RS<()> {
        let shards = self.resolve_queue_shards(queue_oid).await?;
        // Serialize against other cursor mutations of the queue: lock every
        // shard's sentinel in shard order.
        for shard in &shards {
            self.lock_shard_state(tx_mgr.clone(), shard).await?;
        }
        let existing = self.consumer_scan_in_tx(tx_mgr.clone(), queue_oid).await?;
        if existing.iter().any(|entry| entry.name == consumer) {
            return Err(mudu_error!(
                ErrorCode::EntityAlreadyExists,
                format!("consumer {consumer} already exists on queue {queue_oid}")
            ));
        }
        for shard in &shards {
            self.insert_on_partition(
                tx_mgr.clone(),
                QUEUE_CONSUMER_TABLE_ID,
                DEFAULT_UNPARTITIONED_TABLE_PARTITION_ID,
                &consumer_key(queue_oid, shard.shard_index, consumer)?,
                &VecDatum::new(vec![(
                    QUEUE_CONSUMER_FRONTIER_ATTR,
                    queue_consumer_frontier_datum(0)?,
                )]),
            )
            .await?;
        }
        Ok(())
    }

    /// `DROP CONSUMER`: deletes the cursor rows of every shard in the
    /// caller's transaction and publishes the change after commit. Parked
    /// `RECEIVE ... FOR` waiters are woken after commit so a waiter on the
    /// dropped cursor re-scans and fails fast instead of hanging on a
    /// `WAIT FOREVER`.
    pub(crate) async fn consumer_drop_async(
        &self,
        tx_mgr: Arc<dyn TxMgr>,
        queue_oid: OID,
        consumer: &str,
    ) -> RS<()> {
        let shards = self.resolve_queue_shards(queue_oid).await?;
        for shard in &shards {
            self.lock_shard_state(tx_mgr.clone(), shard).await?;
        }
        let mut deleted = 0;
        for shard in &shards {
            deleted += self
                .delete_on_partition(
                    tx_mgr.clone(),
                    QUEUE_CONSUMER_TABLE_ID,
                    DEFAULT_UNPARTITIONED_TABLE_PARTITION_ID,
                    &consumer_key(queue_oid, shard.shard_index, consumer)?,
                )
                .await?;
        }
        if deleted == 0 {
            return Err(mudu_error!(
                ErrorCode::EntityNotFound,
                format!("no such consumer {consumer} on queue {queue_oid}")
            ));
        }
        Ok(())
    }

    /// `ACK q FOR cg TO seq`: advances the cursor frontier of the shard to
    /// `max(frontier, local(seq) + 1)` in the caller's transaction under the
    /// shard state lock, then garbage-collects the shard: rows with a local
    /// `__seq` below the smallest cursor frontier — acknowledged by every
    /// consumer of the shard — are physically deleted. Returns the number of
    /// queue rows the GC deleted.
    pub(crate) async fn consumer_ack_and_gc_async(
        &self,
        tx_mgr: Arc<dyn TxMgr>,
        shard: &QueueShard,
        consumer: &str,
        local_seq: u64,
    ) -> RS<u64> {
        self.lock_shard_state(tx_mgr.clone(), shard).await?;
        self.ensure_shard_runtime_loaded(shard).await?;
        self.ensure_consumer_state_loaded(shard.queue_oid, shard.shard_index)
            .await?;
        let frontier = self
            .consumer_frontier_in_tx(tx_mgr.clone(), shard, consumer)
            .await?
            .map(|current| current.max(local_seq.saturating_add(1)))
            .ok_or_else(|| {
                mudu_error!(
                    ErrorCode::EntityNotFound,
                    format!("no such consumer {consumer} on queue {}", shard.queue_oid)
                )
            })?;
        let updated = self
            .update_on_partition(
                tx_mgr.clone(),
                QUEUE_CONSUMER_TABLE_ID,
                DEFAULT_UNPARTITIONED_TABLE_PARTITION_ID,
                &consumer_key(shard.queue_oid, shard.shard_index, consumer)?,
                &VecDatum::new(vec![(
                    QUEUE_CONSUMER_FRONTIER_ATTR,
                    queue_consumer_frontier_datum(frontier)?,
                )]),
                &OptUpdate::default(),
            )
            .await?;
        if updated == 0 {
            // The cursor was read but its row is gone (a concurrent
            // drop won the race): report the cursor as missing.
            return Err(mudu_error!(
                ErrorCode::EntityNotFound,
                format!("no such consumer {consumer} on queue {}", shard.queue_oid)
            ));
        }
        self.gc_consumer_acked_prefix_async(tx_mgr, shard).await
    }

    /// Physically deletes the shard rows every consumer of the shard has
    /// acknowledged (local `__seq` below the smallest cursor frontier) in
    /// `tx_mgr`, and restages the shard watermark in step so the enqueue
    /// depth cap keeps reflecting live rows. Returns the number of deleted
    /// rows.
    async fn gc_consumer_acked_prefix_async(
        &self,
        tx_mgr: Arc<dyn TxMgr>,
        shard: &QueueShard,
    ) -> RS<u64> {
        let Some(min_frontier) = self
            .consumer_scan_in_tx(tx_mgr.clone(), shard.queue_oid)
            .await?
            .into_iter()
            .filter(|entry| entry.shard == shard.shard_index)
            .map(|entry| entry.frontier)
            .min()
        else {
            return Ok(0);
        };
        if min_frontier == 0 {
            return Ok(0);
        }
        // Highest local sequence acknowledged by every consumer of the shard.
        let gc_local_seq = min_frontier - 1;
        let desc = self.meta_mgr.get_table_by_id(shard.queue_oid).await?;
        let cursor = self
            .read_range_on_partition(
                tx_mgr.clone(),
                shard.queue_oid,
                shard.partition_id,
                &RangeData::new(
                    Bound::Unbounded,
                    Bound::Included(vec![(
                        QUEUE_SEQ_ATTR,
                        queue_seq_datum(queue_seq_encode(shard.shard_index, gc_local_seq)?)?,
                    )]),
                ),
                &VecSelTerm::new((0..desc.fields().len()).collect()),
                &OptRead::default(),
            )
            .await?;
        let mut deleted = 0u64;
        let mut deleted_bytes = 0u64;
        while let Some(row) = cursor.next().await? {
            let fields = row.into_fields();
            let Some(seq_bin) = fields[QUEUE_SEQ_ATTR].clone() else {
                continue;
            };
            let seq = decode_queue_seq(&seq_bin)?;
            if queue_seq_local(seq) == QUEUE_SEQ_LOCAL_SENTINEL {
                continue;
            }
            deleted_bytes += fields
                .iter()
                .skip(QUEUE_PAYLOAD_ATTR_START)
                .flatten()
                .map(|bin| bin.len() as u64)
                .sum::<u64>();
            deleted += self
                .delete_on_partition(
                    tx_mgr.clone(),
                    shard.queue_oid,
                    shard.partition_id,
                    &VecDatum::new(vec![(QUEUE_SEQ_ATTR, seq_bin)]),
                )
                .await? as u64;
        }
        // Keep the shard watermark (frontier + live-depth accounting) aligned
        // with the GC deletes.
        self.ack_queue_state(tx_mgr, shard, gc_local_seq, deleted, deleted_bytes)
            .await?;
        Ok(deleted)
    }

    /// Deletes every `_queue_consumer` row of the dropped queue `queue_oid`
    /// (any shard) in a best-effort internal transaction (DDL is
    /// non-transactional, like the rest of the drop-queue chain), and removes
    /// the legacy pre-sentinel watermark record when present. On a
    /// non-serving replica the commit is rejected; the serving leader's own
    /// cleanup replicates, so the rejection is safe to ignore.
    pub(crate) async fn drop_queue_consumers_async(&self, queue_oid: OID) -> RS<()> {
        let tx_mgr = self._begin_tx()?;
        let consumers = self.consumer_scan_in_tx(tx_mgr.clone(), queue_oid).await?;
        let legacy_key = crate::queue_state::legacy_state_key(queue_oid);
        let has_legacy = self.storage.kv_get(&legacy_key, None).await?.is_some();
        if consumers.is_empty() && !has_legacy {
            let _ = self.worker_rollback_tx(tx_mgr);
            return Ok(());
        }
        tx_mgr.delete(legacy_key);
        for entry in &consumers {
            self.delete_on_partition(
                tx_mgr.clone(),
                QUEUE_CONSUMER_TABLE_ID,
                DEFAULT_UNPARTITIONED_TABLE_PARTITION_ID,
                &consumer_key(queue_oid, entry.shard, &entry.name)?,
            )
            .await?;
        }
        match self.worker_commit_routed_tx_async(tx_mgr).await {
            Ok(()) => Ok(()),
            Err(err) if err.ec() == ErrorCode::DomainViolation => {
                debug!(
                    queue_oid,
                    "queue consumer cleanup skipped: this worker is not the serving leader"
                );
                Ok(())
            }
            Err(err) => Err(err),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn load_register_unregister_round_trip() {
        let registry = ConsumerRegistry::new();
        assert!(!registry.is_loaded(7, 0).unwrap());
        registry
            .load(7, 0, vec![("a".to_string(), 3), ("b".to_string(), 5)])
            .unwrap();
        assert!(registry.is_loaded(7, 0).unwrap());
        assert!(registry.exists(7, 0).unwrap());
        assert!(registry.exists_queue(7).unwrap());
        assert!(!registry.exists_queue(8).unwrap());
        assert_eq!(registry.frontier(7, 0, "a").unwrap(), Some(3));
        assert_eq!(registry.frontier(7, 0, "missing").unwrap(), None);
        assert_eq!(registry.min_frontier(7, 0).unwrap(), Some(3));
        assert_eq!(
            registry.list(7, 0).unwrap(),
            vec![("a".to_string(), 3), ("b".to_string(), 5)]
        );

        registry.register(7, 0, "c").unwrap();
        assert_eq!(registry.frontier(7, 0, "c").unwrap(), Some(0));
        // Re-registering resets a phantom entry left by an aborted create.
        registry.register(7, 0, "a").unwrap();
        assert_eq!(registry.frontier(7, 0, "a").unwrap(), Some(0));

        registry.unregister(7, 0, "a").unwrap();
        assert_eq!(registry.frontier(7, 0, "a").unwrap(), None);
        assert!(registry.exists(7, 0).unwrap());
        registry.unregister(7, 0, "b").unwrap();
        registry.unregister(7, 0, "c").unwrap();
        // The shard's state stays loaded; it just holds no cursors.
        assert!(registry.is_loaded(7, 0).unwrap());
        assert!(!registry.exists(7, 0).unwrap());
        assert_eq!(registry.min_frontier(7, 0).unwrap(), None);
    }

    #[test]
    fn advance_frontier_is_monotonic_under_the_lock() {
        let registry = ConsumerRegistry::new();
        registry.register(7, 1, "a").unwrap();
        assert_eq!(registry.advance_frontier(7, 1, "a", 6).unwrap(), Some(6));
        // Moving backwards merges to the current value.
        assert_eq!(registry.advance_frontier(7, 1, "a", 4).unwrap(), Some(6));
        assert_eq!(registry.frontier(7, 1, "a").unwrap(), Some(6));
        // An unknown cursor reports None.
        assert_eq!(registry.advance_frontier(7, 1, "missing", 1).unwrap(), None);
        assert_eq!(registry.advance_frontier(8, 1, "a", 1).unwrap(), None);
    }

    #[test]
    fn unregister_queue_and_clear_drop_state() {
        let registry = ConsumerRegistry::new();
        registry.register(7, 0, "a").unwrap();
        registry.register(8, 3, "b").unwrap();
        registry.unregister_queue(7).unwrap();
        assert!(!registry.is_loaded(7, 0).unwrap());
        assert!(registry.is_loaded(8, 3).unwrap());
        registry.clear().unwrap();
        assert!(!registry.is_loaded(8, 3).unwrap());
    }
}
