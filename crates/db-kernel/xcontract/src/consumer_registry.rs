//! Per-worker cursor cache of queue-shard consumers.
//!
//! Durable cursor state lives in the `_queue_consumer` system table; this
//! registry mirrors it per worker. Kept as a leaf module (external deps only)
//! so [`crate::contract::WorkerXContract`] can hold it without pulling the
//! queue-consumer business module into the module cycle.

use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu_sys::sync::SMutex;
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
