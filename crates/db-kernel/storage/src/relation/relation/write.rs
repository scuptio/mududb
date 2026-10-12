use super::*;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::atomic::Ordering;

use mudu::common::id::TupleID;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;

use ::catalog::contract::data_row::DataRow;
use ::catalog::contract::table_desc::TableDesc;
use ::catalog::contract::timestamp::Timestamp;
use ::catalog::contract::version_tuple::VersionTuple;
use ::index::index_key::key_tuple::KeyTuple;
use ::xengine::api::{DeltaAssign, VecDatum};
use mudu_utils::scoped_task_trace;

impl RelationInner {
    pub(crate) async fn write_rows(&self, rows: &[(Vec<u8>, Option<Vec<u8>>)], xid: u64) -> RS<()> {
        scoped_task_trace!();
        if rows.is_empty() {
            return Ok(());
        }
        let timestamp = Timestamp::new(xid, u64::MAX);
        let resolved = self.resolve_row_tuples(rows).await?;

        // The key-file and value-file batch inserts are independent; poll
        // them together so their awaited page reads/writes overlap on the
        // current-thread worker instead of running back to back. Each file
        // appends its whole batch to the PL WAL exactly once.
        let key_rows: Vec<(u64, u64, &[u8])> = rows
            .iter()
            .zip(resolved.iter())
            .map(|((key, _), (_, _, tuple_id))| {
                (timestamp.c_min(), *tuple_id as u64, key.as_slice())
            })
            .collect();
        let value_rows: Vec<(u64, u64, &[u8])> = rows
            .iter()
            .zip(resolved.iter())
            .filter_map(|((_, value), (_, _, tuple_id))| {
                value
                    .as_ref()
                    .map(|value| (timestamp.c_min(), *tuple_id as u64, value.as_slice()))
            })
            .collect();
        let (key_result, value_result) = futures::join!(
            self.key_file.insert_batch(&key_rows),
            self.value_file.insert_batch(&value_rows)
        );
        key_result?;
        value_result?;

        Self::publish_row_versions(&timestamp, rows, resolved).await
    }

    /// Resolves the DataRow / tuple id of every key up front so the file
    /// writes can carry their (timestamp, tuple_id, payload) records. The
    /// stripes are held ONLY for this get-or-insert reservation: that is the
    /// section that must stay atomic so two racing writers of a brand-new key
    /// cannot allocate two tuple ids (see the `write_stripes` field comment).
    /// The file writes are serialized by the files' own write latches, and
    /// the version publishes are per-row synchronized through `DataRow`, so
    /// neither needs the stripes — holding them across the whole
    /// check-then-act used to serialize concurrent batches for entire
    /// file-write rounds. Stripes are taken in ascending index order so
    /// concurrent batches cannot deadlock, and single-row writers take their
    /// one stripe through the same array.
    async fn resolve_row_tuples<V>(
        &self,
        rows: &[(Vec<u8>, V)],
    ) -> RS<Vec<(KeyTuple, DataRow, u128)>> {
        let mut stripe_indices: Vec<usize> = rows
            .iter()
            .map(|(key, _)| Self::stripe_index(key))
            .collect();
        stripe_indices.sort_unstable();
        stripe_indices.dedup();
        let mut resolved = Vec::with_capacity(rows.len());
        {
            let _stage = ::catalog::stage_stats::StageGuard::new(
                ::catalog::stage_stats::Stage::WrStripeWait,
            );
            let mut _stripe_guards = Vec::with_capacity(stripe_indices.len());
            for index in stripe_indices {
                _stripe_guards.push(self.write_stripes[index].lock().await);
            }
            let _stage =
                ::catalog::stage_stats::StageGuard::new(::catalog::stage_stats::Stage::WrResolve);
            for (key, _) in rows {
                let key_tuple = KeyTuple::from(key.clone());
                let row = match self.index.get(&key_tuple)? {
                    Some(row) => row,
                    None => {
                        let tuple_id = self.alloc_tuple_id();
                        let row = DataRow::new(tuple_id);
                        // Publish the reservation immediately so a racing
                        // batch resolves this same row instead of allocating
                        // a second tuple id. The row is still invisible to
                        // readers until a version is written (an empty
                        // version chain reads as absent), which matches the
                        // old publish-after-persist visibility.
                        let _ = self.index.insert(key_tuple.clone(), row.clone())?;
                        row
                    }
                };
                let tuple_id = row
                    .tuple_id()
                    .await?
                    .ok_or_else(|| mudu_error!(ErrorCode::Internal, "missing tuple id"))?;
                resolved.push((key_tuple, row, tuple_id));
            }
        }
        Ok(resolved)
    }

    /// Publishes each row's committed payload as its newest in-memory version
    /// so visibility reads can skip the value file's page chain; the value
    /// file record written above stays the durable source for rebuilds and
    /// for versions evicted from the retained window. The shallow write keeps
    /// the append-only delta chain metadata-only (no per-write payload
    /// clone).
    async fn publish_row_versions(
        timestamp: &Timestamp,
        rows: &[(Vec<u8>, Option<Vec<u8>>)],
        resolved: Vec<(KeyTuple, DataRow, u128)>,
    ) -> RS<()> {
        let _stage =
            ::catalog::stage_stats::StageGuard::new(::catalog::stage_stats::Stage::WrRowIndex);
        for ((_, value), (_, row, _)) in rows.iter().zip(resolved) {
            let version = match value {
                Some(value) => VersionTuple::new(timestamp.clone(), value.clone()),
                None => VersionTuple::new_delete(timestamp.clone()),
            };
            row.write_shallow(version).await?;
        }
        Ok(())
    }

    /// Deferred (lock-free) delta apply for one batch of rows. Each row's
    /// new value is computed against the latest committed version atomically
    /// under the row lock (`DataRow::apply_update_to_latest_sync`), so
    /// concurrent commutative updates never overwrite each other; tuple-id
    /// resolution stays serialized by the stripes exactly as in
    /// [`Self::write_rows`]. The caller must guarantee that every concurrent
    /// writer of these keys uses commutative deferred deltas.
    pub(crate) async fn write_rows_delta(
        &self,
        desc: &TableDesc,
        rows: &[(Vec<u8>, Vec<DeltaAssign>)],
        xid: u64,
    ) -> RS<()> {
        scoped_task_trace!();
        if rows.is_empty() {
            return Ok(());
        }
        let timestamp = Timestamp::new(xid, u64::MAX);
        let resolved = self.resolve_row_tuples(rows).await?;

        // Compute and publish the new version per row. The read-compute-
        // append sequence is atomic under the row lock, so this needs no
        // stripe; file records written below carry the same computed
        // payload, keeping the in-memory version and the durable record
        // identical.
        let mut computed_rows: Vec<(Vec<u8>, Vec<u8>)> = Vec::with_capacity(rows.len());
        for ((key, deltas), (_, row, _)) in rows.iter().zip(resolved.iter()) {
            let computed = row.apply_update_to_latest_sync(timestamp.clone(), |current| {
                crate::value_update::apply_value_update_with_deltas(
                    current,
                    &VecDatum::new(vec![]),
                    deltas,
                    &[],
                    desc,
                )
            })?;
            computed_rows.push((key.clone(), computed));
        }

        let key_rows: Vec<(u64, u64, &[u8])> = rows
            .iter()
            .zip(resolved.iter())
            .map(|((key, _), (_, _, tuple_id))| {
                (timestamp.c_min(), *tuple_id as u64, key.as_slice())
            })
            .collect();
        let value_rows: Vec<(u64, u64, &[u8])> = computed_rows
            .iter()
            .zip(resolved.iter())
            .map(|((_, value), (_, _, tuple_id))| {
                (timestamp.c_min(), *tuple_id as u64, value.as_slice())
            })
            .collect();
        let (key_result, value_result) = futures::join!(
            self.key_file.insert_batch(&key_rows),
            self.value_file.insert_batch(&value_rows)
        );
        key_result?;
        value_result?;
        Ok(())
    }

    fn stripe_index(key: &[u8]) -> usize {
        let mut hasher = DefaultHasher::new();
        key.hash(&mut hasher);
        (hasher.finish() as usize) % WRITE_STRIPE_COUNT
    }

    fn alloc_tuple_id(&self) -> TupleID {
        self.next_tuple_id.fetch_add(1, Ordering::Relaxed)
    }
}
