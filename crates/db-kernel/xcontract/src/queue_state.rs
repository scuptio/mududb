//! Transactional queue-shard watermarks, stored as **sentinel rows** inside
//! the queue relation's shard partitions.
//!
//! Each shard owns one sentinel row: its key is the shard's reserved local
//! sequence [`catalog::contract::queue::QUEUE_SEQ_LOCAL_SENTINEL`], its `status` is
//! [`catalog::contract::queue::QUEUE_STATUS_STATE`], and the four counters (`next`, `frontier`,
//! `depth`, `bytes`) are packed into the hidden `__state_hi`/`__state_lo`
//! columns. Because the watermark is an ordinary relation row, reserve/ack
//! stay pure relation-write transactions and compose with the local,
//! handoff, and cross-partition commit paths — including from transactions
//! coordinated on a worker that does not own the shard.
//!
//! Legacy databases (pre-sentinel) stored the watermark in the worker KV
//! under `\0mudu.queue.state.v1/<oid>`; the owner-side rebuild reads that
//! record once and re-homes it into the sentinel row.
use super::*;
use crate::utils::{build_key_tuple, build_value_tuple, project_selected_fields};
use ::catalog::contract::queue::{
    decode_queue_seq, decode_queue_state_word, queue_lsn_datum, queue_receive_count_datum,
    queue_sentinel_seq, queue_seq_datum, queue_seq_local, queue_state_word_datum,
    queue_status_datum, QUEUE_LSN_ATTR, QUEUE_RECEIVE_COUNT_ATTR, QUEUE_SEQ_ATTR,
    QUEUE_SEQ_LOCAL_MAX, QUEUE_SEQ_LOCAL_SENTINEL, QUEUE_STATE_HI_ATTR, QUEUE_STATE_LO_ATTR,
    QUEUE_STATUS_ATTR, QUEUE_STATUS_STATE,
};

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

/// The four transactional watermark counters of one queue shard.
#[derive(Default)]
pub(crate) struct QueueState {
    pub next: u64,
    pub frontier: u64,
    pub depth: u64,
    pub bytes: u64,
}

impl QueueState {
    /// Legacy v1 KV encoding (33 bytes: version + four words); kept to read
    /// pre-sentinel databases during the owner-side rebuild.
    fn decode_legacy(bytes: &[u8]) -> RS<Self> {
        if bytes.len() != 33 || bytes[0] != 1 {
            return Err(mudu_error!(ErrorCode::Decode, "invalid queue state v1"));
        }
        let mut values = [0u64; 4];
        for (value, part) in values.iter_mut().zip(bytes[1..].as_chunks::<8>().0.iter()) {
            let mut array = [0; 8];
            array.copy_from_slice(part);
            *value = u64::from_be_bytes(array);
        }
        Ok(Self {
            next: values[0],
            frontier: values[1],
            depth: values[2],
            bytes: values[3],
        })
    }
}

/// The key datum of the sentinel row of shard `shard_index`.
pub(crate) fn sentinel_key_datum(shard_index: u64) -> RS<VecDatum> {
    Ok(VecDatum::new(vec![(
        QUEUE_SEQ_ATTR,
        queue_seq_datum(queue_sentinel_seq(shard_index)?)?,
    )]))
}

/// The physical key tuple of the sentinel row of shard `shard_index`.
pub(crate) fn sentinel_key_tuple(desc: &TableDesc, shard_index: u64) -> RS<Vec<u8>> {
    build_key_tuple(&sentinel_key_datum(shard_index)?, desc)
}

/// Whether the physical row key `key` of the queue `desc` addresses a
/// sentinel row; used by the commit path to spot watermark writes.
pub(crate) fn is_sentinel_key(desc: &TableDesc, key: &[u8]) -> RS<bool> {
    let field = desc.key_desc().get_field_desc(0).get(key)?;
    let seq = decode_queue_seq(field)?;
    Ok(queue_seq_local(seq) == QUEUE_SEQ_LOCAL_SENTINEL)
}

/// Reads the watermark counters from a sentinel row value; `None` when the
/// row carries no state words.
pub(crate) fn parse_sentinel_value(
    desc: &TableDesc,
    key: &[u8],
    value: &[u8],
) -> RS<Option<QueueState>> {
    let select = VecSelTerm::new(vec![QUEUE_STATE_HI_ATTR, QUEUE_STATE_LO_ATTR]);
    let fields = project_selected_fields(desc, key, value, &select)?;
    let (Some(hi), Some(lo)) = (&fields[0], &fields[1]) else {
        return Ok(None);
    };
    let (next, frontier) = decode_queue_state_word(hi)?;
    let (depth, bytes) = decode_queue_state_word(lo)?;
    Ok(Some(QueueState {
        next,
        frontier,
        depth,
        bytes,
    }))
}

/// Builds the sentinel row value holding `state` (the `__lsn` column carries
/// the writing transaction's xid, like message rows).
fn sentinel_value(desc: &TableDesc, lsn: u64, state: &QueueState) -> RS<Vec<u8>> {
    build_value_tuple(
        &VecDatum::new(vec![
            (QUEUE_LSN_ATTR, queue_lsn_datum(lsn)?),
            (QUEUE_STATUS_ATTR, queue_status_datum(QUEUE_STATUS_STATE)?),
            (QUEUE_RECEIVE_COUNT_ATTR, queue_receive_count_datum(0)?),
            (
                QUEUE_STATE_HI_ATTR,
                queue_state_word_datum(state.next, state.frontier)?,
            ),
            (
                QUEUE_STATE_LO_ATTR,
                queue_state_word_datum(state.depth, state.bytes)?,
            ),
        ]),
        desc,
    )
}

impl WorkerXContract {
    /// Serializes the shard's watermark mutations until `tx` ends: the
    /// statement lock on the sentinel row key (taken on the owner, remotely
    /// when the shard is remote).
    pub(crate) async fn lock_shard_state(&self, tx: Arc<dyn TxMgr>, shard: &QueueShard) -> RS<()> {
        let desc = self.meta_mgr.get_table_by_id(shard.queue_oid).await?;
        let key = sentinel_key_tuple(&desc, shard.shard_index)?;
        self.lock_relation_key(tx.as_ref(), shard, key).await
    }

    /// Takes the statement lock on one key of the shard's partition, locally
    /// or on the owning worker.
    pub(crate) async fn lock_relation_key(
        &self,
        tx: &dyn TxMgr,
        shard: &QueueShard,
        key: Vec<u8>,
    ) -> RS<()> {
        let relation_id = PhysicalRelationId {
            table_id: shard.queue_oid,
            partition_id: shard.partition_id,
        };
        if shard.worker_id == self.worker_id {
            self.acquire_statement_lock(tx, relation_id, key).await
        } else {
            self.lock_remote_key_for_update(
                tx,
                shard.worker_id,
                shard.queue_oid,
                shard.partition_id,
                key,
                vec![],
            )
            .await?;
            Ok(())
        }
    }

    /// Reads the shard watermark with the transaction's overlay: the caller's
    /// own staged sentinel write wins, then the committed sentinel row (read
    /// under the statement lock). A shard without a sentinel row starts
    /// empty; the owner-side prepare migrates pre-sentinel databases before
    /// any reserve/ack runs.
    pub(crate) async fn read_shard_state(
        &self,
        tx: Arc<dyn TxMgr>,
        shard: &QueueShard,
    ) -> RS<QueueState> {
        let desc = self.meta_mgr.get_table_by_id(shard.queue_oid).await?;
        let relation_id = PhysicalRelationId {
            table_id: shard.queue_oid,
            partition_id: shard.partition_id,
        };
        let key = sentinel_key_tuple(&desc, shard.shard_index)?;
        if let Some(Some(value)) = tx.get_relation(relation_id, &key) {
            if let Some(state) = parse_sentinel_value(&desc, &key, &value)? {
                return Ok(state);
            }
        }
        let fields = if shard.worker_id == self.worker_id {
            self.acquire_statement_lock(tx.as_ref(), relation_id, key.clone())
                .await?;
            let value = self
                .storage
                .get_on_partition(shard.queue_oid, Some(shard.partition_id), &key, tx.as_ref())
                .await?;
            match value {
                Some(value) => project_selected_fields(
                    &desc,
                    &key,
                    &value,
                    &VecSelTerm::new(vec![QUEUE_STATE_HI_ATTR, QUEUE_STATE_LO_ATTR]),
                )?,
                None => return Ok(QueueState::default()),
            }
        } else {
            let fields = self
                .lock_remote_key_for_update(
                    tx.as_ref(),
                    shard.worker_id,
                    shard.queue_oid,
                    shard.partition_id,
                    key,
                    vec![QUEUE_STATE_HI_ATTR, QUEUE_STATE_LO_ATTR],
                )
                .await?;
            match fields {
                Some(fields) => fields,
                None => return Ok(QueueState::default()),
            }
        };
        let (Some(hi), Some(lo)) = (&fields[0], &fields[1]) else {
            return Ok(QueueState::default());
        };
        let (next, frontier) = decode_queue_state_word(hi)?;
        let (depth, bytes) = decode_queue_state_word(lo)?;
        Ok(QueueState {
            next,
            frontier,
            depth,
            bytes,
        })
    }

    /// Committed-only read of the shard sentinel for the owner-side rebuild
    /// (no transaction is taken: a throwaway read-only one is used).
    pub(crate) async fn read_shard_sentinel_committed(
        &self,
        shard: &QueueShard,
    ) -> RS<Option<QueueState>> {
        self.ensure_local_shard_owner(shard)?;
        let desc = self.meta_mgr.get_table_by_id(shard.queue_oid).await?;
        let key = sentinel_key_tuple(&desc, shard.shard_index)?;
        let tx = self._begin_tx()?;
        let value = self
            .storage
            .get_on_partition(shard.queue_oid, Some(shard.partition_id), &key, tx.as_ref())
            .await;
        let _ = self.worker_rollback_tx(tx);
        let value = value?;
        match value {
            Some(value) => parse_sentinel_value(&desc, &key, &value),
            None => Ok(None),
        }
    }

    /// Reads the legacy pre-sentinel watermark of an unpartitioned queue from
    /// the worker KV, when present.
    pub(crate) async fn read_legacy_state(&self, queue_oid: OID) -> RS<Option<QueueState>> {
        let bytes = self
            .storage
            .kv_get(&legacy_state_key(queue_oid), None)
            .await?;
        bytes
            .map(|bytes| QueueState::decode_legacy(&bytes))
            .transpose()
    }

    /// Stages the shard watermark row in `tx` (absolute values; idempotent
    /// under the cross-partition apply replay).
    pub(crate) fn stage_shard_state(
        &self,
        tx: Arc<dyn TxMgr>,
        shard: &QueueShard,
        desc: &TableDesc,
        state: &QueueState,
    ) -> RS<()> {
        let key = sentinel_key_tuple(desc, shard.shard_index)?;
        let value = sentinel_value(desc, tx.xid(), state)?;
        tx.put_relation(
            PhysicalRelationId {
                table_id: shard.queue_oid,
                partition_id: shard.partition_id,
            },
            key,
            value,
        );
        Ok(())
    }

    /// Reserves `count` local sequences and `bytes` payload bytes of `shard`
    /// in the enqueue transaction, returning the first local sequence.
    pub(crate) async fn reserve_queue(
        &self,
        tx: Arc<dyn TxMgr>,
        shard: &QueueShard,
        count: u64,
        bytes: u64,
    ) -> RS<u64> {
        let mut state = self.read_shard_state(tx.clone(), shard).await?;
        let (max_msgs, max_bytes) = self.queue_registry.limits()?;
        if state.depth.saturating_add(count) > max_msgs
            || state.bytes.saturating_add(bytes) > max_bytes
        {
            if shard.worker_id == self.worker_id {
                self.queue_registry.record_overflow(shard_key(shard))?;
            }
            return Err(mudu_error!(
                ErrorCode::QueueFull,
                "queue message or byte limit exceeded"
            ));
        }
        let first = state.next;
        state.next = state
            .next
            .checked_add(count)
            .filter(|next| *next <= QUEUE_SEQ_LOCAL_MAX)
            .ok_or_else(|| mudu_error!(ErrorCode::InvalidState, "queue sequence exhausted"))?;
        state.depth += count;
        state.bytes += bytes;
        let desc = self.meta_mgr.get_table_by_id(shard.queue_oid).await?;
        self.stage_shard_state(tx, shard, &desc, &state)?;
        Ok(first)
    }

    /// Restages the shard watermark after a delete round of `deleted` rows
    /// (`deleted_bytes` encoded payload bytes), advancing the acked frontier
    /// past `local_seq`.
    pub(crate) async fn ack_queue_state(
        &self,
        tx: Arc<dyn TxMgr>,
        shard: &QueueShard,
        local_seq: u64,
        deleted: u64,
        deleted_bytes: u64,
    ) -> RS<()> {
        let mut state = self.read_shard_state(tx.clone(), shard).await?;
        state.frontier = state
            .frontier
            .max(local_seq.saturating_add(1))
            .min(state.next);
        state.depth = state.depth.saturating_sub(deleted);
        state.bytes = state.bytes.saturating_sub(deleted_bytes);
        let desc = self.meta_mgr.get_table_by_id(shard.queue_oid).await?;
        self.stage_shard_state(tx, shard, &desc, &state)?;
        Ok(())
    }

    /// Publishes committed watermark counters to the in-memory registry
    /// (observability/activation mirror).
    pub(crate) fn publish_queue_state(
        &self,
        shard: QueueShardId,
        next: u64,
        frontier: u64,
        depth: u64,
        bytes: u64,
    ) -> RS<()> {
        self.queue_registry
            .set_durable(shard, next, frontier, depth, bytes)
    }

    /// Extracts the shard watermarks staged in `tx`: every queue-relation
    /// sentinel-row write becomes one `(shard, next, frontier, depth, bytes)`
    /// entry, published to the in-memory registry after the commit applies.
    pub(crate) async fn staged_shard_states(
        &self,
        tx: &dyn TxMgr,
    ) -> RS<Vec<(QueueShardId, u64, u64, u64, u64)>> {
        let staged = tx.staged_relation_ops();
        if staged.is_empty() {
            return Ok(Vec::new());
        }
        let mut out = Vec::new();
        for (relation_id, rows) in staged {
            let Ok(desc) = self.meta_mgr.get_table_by_id(relation_id.table_id).await else {
                continue;
            };
            if !desc.is_queue() {
                continue;
            }
            for (key, value) in &rows {
                let Some(value) = value else { continue };
                if !is_sentinel_key(&desc, key)? {
                    continue;
                }
                if let Some(state) = parse_sentinel_value(&desc, key, value)? {
                    out.push((
                        (relation_id.table_id, relation_id.partition_id),
                        state.next,
                        state.frontier,
                        state.depth,
                        state.bytes,
                    ));
                }
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_state_v1_matches_golden_and_rejects_invalid_frames() -> RS<()> {
        let golden = include_bytes!("../../testing/fixtures/golden/v1/queue_state_v1.bin");
        let decoded = QueueState::decode_legacy(golden)?;
        assert_eq!(decoded.next, 9);
        assert_eq!(decoded.frontier, 4);
        assert_eq!(decoded.depth, 5);
        assert_eq!(decoded.bytes, 123);
        assert!(QueueState::decode_legacy(&golden[..32]).is_err());
        let mut future = *golden;
        future[0] = 2;
        assert!(QueueState::decode_legacy(&future).is_err());
        assert_eq!(legacy_state_oid(&legacy_state_key(42)), Some(42));
        assert_eq!(legacy_state_oid(b"user-key"), None);
        Ok(())
    }
}
