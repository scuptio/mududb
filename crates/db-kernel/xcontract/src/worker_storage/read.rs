use std::collections::BTreeMap;
use std::ops::Bound;
use std::ops::Bound::{Excluded, Included, Unbounded};

use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu_utils::task_trace;

use ::catalog::contract::queue::{
    decode_queue_seq, decode_queue_status, QUEUE_STATUS_ATTR, QUEUE_STATUS_CLAIMED,
    QUEUE_STATUS_STATE,
};
use ::catalog::contract::worker_snapshot::{KvItem, WorkerSnapshot};
use ::index::index_key::key_tuple::KeyTuple;
use ::meta::queue_consumer::{
    decode_queue_consumer_frontier, decode_queue_consumer_name, decode_queue_consumer_oid,
    queue_consumer_desc, QUEUE_CONSUMER_FRONTIER_ATTR, QUEUE_CONSUMER_NAME_ATTR,
    QUEUE_CONSUMER_OID_ATTR, QUEUE_CONSUMER_TABLE_ID,
};
use ::xengine::api::VecSelTerm;
use ::xengine::partition_router::DEFAULT_UNPARTITIONED_TABLE_PARTITION_ID;
use ::xengine::tx_mgr::TxMgr;

use super::{QueueRebuildScan, WorkerStorage};

impl WorkerStorage {
    #[cfg(test)]
    pub(crate) async fn contains_key(&self, oid: OID, key: &KeyTuple, txm: &dyn TxMgr) -> RS<bool> {
        self.contains_key_on_partition(oid, None, key, txm).await
    }

    #[cfg(test)]
    pub(crate) async fn contains_key_on_partition(
        &self,
        oid: OID,
        partition_id: Option<OID>,
        key: &KeyTuple,
        txm: &dyn TxMgr,
    ) -> RS<bool> {
        let relation_id = self.relation_id(oid, self.physical_partition_id(partition_id));
        if let Some(staged) = txm.get_relation(relation_id, key.as_slice()) {
            return Ok(staged.is_some());
        }
        self.read_visible_relation_exists(oid, partition_id, key, &txm.snapshot())
            .await
    }

    #[cfg(test)]
    /// Read `key` of relation `oid` on the default partition as visible to
    /// `txm` (test helper).
    pub async fn get(&self, oid: OID, key: &[u8], txm: &dyn TxMgr) -> RS<Option<Vec<u8>>> {
        self.get_on_partition(oid, None, key, txm).await
    }

    /// Read `key` of relation `oid` on `partition_id` (the default partition
    /// when `None`) as visible to `txm`, merging the transaction's staged
    /// writes over the latest committed version (read-your-writes; the
    /// committed read is READ COMMITTED).
    pub async fn get_on_partition(
        &self,
        oid: OID,
        partition_id: Option<OID>,
        key: &[u8],
        txm: &dyn TxMgr,
    ) -> RS<Option<Vec<u8>>> {
        let trace = task_trace!();
        let relation_id = self.relation_id(oid, self.physical_partition_id(partition_id));
        trace.watch("storage.get.table_id", &oid.to_string());
        trace.watch("storage.get.relation_id", &format!("{relation_id:?}"));
        trace.watch(
            "storage.get.partition_id",
            &partition_id
                .map(|id| id.to_string())
                .unwrap_or_else(|| "none".to_string()),
        );
        trace.watch("storage.get.stage", "tx_lookup_call");
        if let Some(staged) = txm.get_relation(relation_id, key) {
            trace.watch("storage.get.stage", "tx_lookup_hit");
            return Ok(staged);
        }
        // Read-your-writes for deferred deltas: fold the staged (not yet
        // applied) assignments over the latest committed value without
        // publishing anything.
        if let Some(deltas) = txm.get_relation_deltas(relation_id, key) {
            let desc = self.mgr.get_table_by_id(oid).await?;
            let snapshot = WorkerSnapshot::latest_committed();
            let key_tuple = KeyTuple::from(key.to_vec());
            let current = self
                .read_visible_relation_value(oid, partition_id, &key_tuple, &snapshot)
                .await?;
            return match current {
                Some(value) => Ok(Some(
                    ::storage::value_update::apply_value_update_with_deltas(
                        &value,
                        &::xengine::api::VecDatum::new(vec![]),
                        &deltas,
                        &[],
                        desc.as_ref(),
                    )?,
                )),
                None => Ok(None),
            };
        }
        trace.watch("storage.get.stage", "tx_lookup_miss");
        let key = KeyTuple::from(key.to_vec());
        trace.watch("storage.get.stage", "visible_read");
        // Relation reads are READ COMMITTED (postgres parity): every
        // statement observes the latest committed version rather than the
        // transaction's begin snapshot. Staged overlays above preserve
        // read-your-writes; write-write conflict protection for staged keys
        // comes from the statement/commit locks, not from this snapshot.
        let snapshot = WorkerSnapshot::latest_committed();
        self.read_visible_relation_value(oid, partition_id, &key, &snapshot)
            .await
    }

    /// Read `key` from relation `oid` on `partition_id` exactly as visible to
    /// `snapshot`, without any transaction's staged overlay. Used by callers
    /// that run outside a transaction (the fs GC).
    pub async fn get_on_partition_with_snapshot(
        &self,
        oid: OID,
        partition_id: Option<OID>,
        key: &[u8],
        snapshot: &WorkerSnapshot,
    ) -> RS<Option<Vec<u8>>> {
        let key = KeyTuple::from(key.to_vec());
        self.read_visible_relation_value(oid, partition_id, &key, snapshot)
            .await
    }

    /// Scan the committed head/tail of the queue relation `table_id` and
    /// return the smallest and largest live `__seq` values; `None` when the
    /// relation is not open locally or holds no live rows.
    ///
    /// Used to rebuild the in-memory queue runtime counters at bootstrap. The
    /// scan materializes the queue's live rows (bounded by the queue depth
    /// cap); acknowledged rows are physically deleted at ACK time, so they
    /// are not scanned.
    /// The `(min_seq, max_seq)` live bounds of the queue shard, when it holds
    /// any message rows.
    pub async fn queue_seq_bounds_async(
        &self,
        table_id: OID,
        partition_id: OID,
    ) -> RS<Option<(u64, u64)>> {
        Ok(self
            .queue_rebuild_scan_async(table_id, partition_id)
            .await?
            .map(|scan| (scan.min_seq, scan.max_seq)))
    }

    /// Scan the committed live rows of the queue shard `table_id` /
    /// `partition_id` for the runtime rebuild: the smallest and largest live
    /// `__seq` plus the sequence numbers of rows still marked `claimed`
    /// (in-flight at crash time; the rebuild re-delivers them). The shard's
    /// sentinel row (`QUEUE_STATUS_STATE`) is excluded. `None` when the
    /// relation partition is not open locally or holds no live message rows.
    pub async fn queue_rebuild_scan_async(
        &self,
        table_id: OID,
        partition_id: OID,
    ) -> RS<Option<QueueRebuildScan>> {
        let relation_id = self.relation_id(table_id, partition_id);
        let relation = match self.get_relation_by_id_async(&relation_id).await {
            Ok(relation) => relation,
            Err(err) if err.ec() == ErrorCode::ObjectNotFound => return Ok(None),
            Err(err) => return Err(err),
        };
        let rows = relation
            .visible_range(
                (Bound::Unbounded, Bound::Unbounded),
                &WorkerSnapshot::latest_committed(),
            )
            .await?;
        let desc = self.mgr.get_table_by_id(table_id).await?;
        // The queue layout guarantees a single key column: `__seq`.
        let seq_field = desc.key_desc().get_field_desc(0);
        let status_select = VecSelTerm::new(vec![QUEUE_STATUS_ATTR]);
        let mut min_seq = u64::MAX;
        let mut max_seq = 0u64;
        let mut claimed_seqs = Vec::new();
        let mut live = 0u64;
        for (key, value) in &rows {
            let seq = decode_queue_seq(seq_field.get(key)?)?;
            let status =
                crate::utils::project_selected_fields(desc.as_ref(), key, value, &status_select)?;
            if let Some(status_bin) = &status[0] {
                let status = decode_queue_status(status_bin)?;
                if status == QUEUE_STATUS_STATE {
                    continue;
                }
                if status == QUEUE_STATUS_CLAIMED {
                    claimed_seqs.push(seq);
                }
            }
            min_seq = min_seq.min(seq);
            max_seq = max_seq.max(seq);
            live += 1;
        }
        if live == 0 {
            return Ok(None);
        }
        Ok(Some(QueueRebuildScan {
            min_seq,
            max_seq,
            claimed_seqs,
        }))
    }

    /// Scan the committed `_queue_consumer` rows of the queue `queue_oid` and
    /// return its consumer cursors as `(name, frontier)` pairs; empty when
    /// the relation is not open locally or the queue has no consumers.
    ///
    /// Used to rebuild the in-memory consumer cursor cache lazily on the
    /// first queue access after a restart. The `_queue_consumer` relation
    /// always lives on [`DEFAULT_UNPARTITIONED_TABLE_PARTITION_ID`] (queues
    /// are never partitioned), the same partition the queue relations use.
    pub async fn consumer_scan_async(&self, queue_oid: OID) -> RS<Vec<(String, u64)>> {
        let relation_id = self.relation_id(
            QUEUE_CONSUMER_TABLE_ID,
            DEFAULT_UNPARTITIONED_TABLE_PARTITION_ID,
        );
        let relation = match self.get_relation_by_id_async(&relation_id).await {
            Ok(relation) => relation,
            Err(err) if err.ec() == ErrorCode::ObjectNotFound => return Ok(Vec::new()),
            Err(err) => return Err(err),
        };
        let rows = relation
            .visible_range(
                (Bound::Unbounded, Bound::Unbounded),
                &WorkerSnapshot::latest_committed(),
            )
            .await?;
        if rows.is_empty() {
            return Ok(Vec::new());
        }
        let desc = queue_consumer_desc()?;
        let select = VecSelTerm::new(vec![
            QUEUE_CONSUMER_OID_ATTR,
            QUEUE_CONSUMER_NAME_ATTR,
            QUEUE_CONSUMER_FRONTIER_ATTR,
        ]);
        let mut entries = Vec::new();
        for (key, value) in &rows {
            let fields = crate::utils::project_selected_fields(desc.as_ref(), key, value, &select)?;
            let (Some(oid_bin), Some(name_bin), Some(frontier_bin)) =
                (fields[0].clone(), fields[1].clone(), fields[2].clone())
            else {
                continue;
            };
            if decode_queue_consumer_oid(&oid_bin)? != queue_oid {
                continue;
            }
            entries.push((
                decode_queue_consumer_name(&name_bin)?,
                decode_queue_consumer_frontier(&frontier_bin)?,
            ));
        }
        Ok(entries)
    }

    pub(crate) async fn range(
        &self,
        oid: OID,
        bounds: (Bound<&[u8]>, Bound<&[u8]>),
        txm: &dyn TxMgr,
    ) -> RS<Vec<(Vec<u8>, Vec<u8>)>> {
        self.range_on_partition(oid, None, bounds, txm).await
    }

    pub(crate) async fn range_on_partition(
        &self,
        oid: OID,
        partition_id: Option<OID>,
        bounds: (Bound<&[u8]>, Bound<&[u8]>),
        txm: &dyn TxMgr,
    ) -> RS<Vec<(Vec<u8>, Vec<u8>)>> {
        // READ COMMITTED (see `get_on_partition`): base items observe the
        // latest committed versions; the transaction's staged overlay is
        // merged on top for read-your-writes.
        let base_items = self
            .range_visible_relation(
                oid,
                partition_id,
                bounds,
                &WorkerSnapshot::latest_committed(),
            )
            .await?;
        let (start_key, end_key) = bounds_to_scan(&bounds);
        let relation_id = self.relation_id(oid, self.physical_partition_id(partition_id));
        let staged_items = txm.staged_relation_items_in_range(relation_id, &start_key, &end_key);

        let mut merged = BTreeMap::new();
        for (key, value) in base_items {
            merged.insert(key, Some(value));
        }
        for (key, value) in staged_items {
            merged.insert(key, value);
        }

        Ok(merged
            .into_iter()
            .filter_map(|(key, value)| value.map(|value| (key, value)))
            .collect())
    }

    pub(crate) async fn kv_get(
        &self,
        key: &[u8],
        snapshot: Option<&WorkerSnapshot>,
    ) -> RS<Option<Vec<u8>>> {
        let row = self.kv_store.get_sync(key).map(|entry| entry.get().clone());
        let version = match snapshot {
            Some(snapshot) => match row {
                Some(row) => {
                    let snapshot = snapshot.to_snapshot();
                    row.read(&snapshot).await?
                }
                None => None,
            },
            None => match row {
                Some(row) => row.read_latest().await?,
                None => None,
            },
        };
        Ok(version
            .filter(|version| !version.is_deleted())
            .map(|version| version.tuple().clone()))
    }

    pub(crate) async fn kv_range(
        &self,
        start_key: &[u8],
        end_key: &[u8],
        snapshot: Option<&WorkerSnapshot>,
    ) -> RS<Vec<KvItem>> {
        let mut rows = Vec::new();
        self.kv_store.iter_sync(|key, row| {
            let in_range = if end_key.is_empty() {
                key.as_slice() >= start_key
            } else {
                key.as_slice() >= start_key && key.as_slice() < end_key
            };
            if in_range {
                rows.push((key.clone(), row.clone()));
            }
            true
        });

        let mut items = Vec::new();
        for (key, row) in rows {
            let visible = match snapshot {
                Some(snapshot) => {
                    let snapshot = snapshot.to_snapshot();
                    row.read(&snapshot).await?
                }
                None => row.read_latest().await?,
            };
            if let Some(visible) = visible.filter(|version| !version.is_deleted()) {
                items.push(KvItem {
                    key,
                    value: visible.tuple().clone(),
                });
            }
        }
        items.sort_by(|left, right| left.key.cmp(&right.key));
        Ok(items)
    }

    #[cfg(test)]
    async fn read_visible_relation_exists(
        &self,
        oid: OID,
        partition_id: Option<OID>,
        key: &KeyTuple,
        snapshot: &WorkerSnapshot,
    ) -> RS<bool> {
        let relation = self.get_relation_async(oid, partition_id).await?;
        relation.has_visible_version(key, snapshot).await
    }

    pub(super) async fn read_visible_relation_value(
        &self,
        oid: OID,
        partition_id: Option<OID>,
        key: &KeyTuple,
        snapshot: &WorkerSnapshot,
    ) -> RS<Option<Vec<u8>>> {
        // Hot path: one relation-store lookup. Only a miss pays for the
        // ensure-then-relookup dance (previously every read paid for both
        // `ensure_relation_index` and `get_relation_async`).
        let relation = match self.get_relation_async(oid, partition_id).await {
            Ok(relation) => relation,
            Err(err) if err.ec() == ErrorCode::ObjectNotFound => {
                self.ensure_relation_index(oid, partition_id).await?;
                self.get_relation_async(oid, partition_id).await?
            }
            Err(err) => return Err(err),
        };
        relation.visible_value(key, snapshot).await
    }

    async fn range_visible_relation(
        &self,
        oid: OID,
        partition_id: Option<OID>,
        bounds: (Bound<&[u8]>, Bound<&[u8]>),
        snapshot: &WorkerSnapshot,
    ) -> RS<Vec<(Vec<u8>, Vec<u8>)>> {
        self.ensure_relation_index(oid, partition_id).await?;
        let relation = self.get_relation_async(oid, partition_id).await?;
        relation.visible_range(bounds, snapshot).await
    }
}

fn bounds_to_scan(bounds: &(Bound<&[u8]>, Bound<&[u8]>)) -> (Vec<u8>, Vec<u8>) {
    let start = match bounds.0 {
        Included(key) | Excluded(key) => key.to_vec(),
        Unbounded => Vec::new(),
    };
    let end = match bounds.1 {
        Included(key) | Excluded(key) => key.to_vec(),
        Unbounded => Vec::new(),
    };
    (start, end)
}
