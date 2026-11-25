//! Executor of a queue `ACK` statement: physically removes the claimed
//! prefix of one queue shard and restages the shard's watermark.
//!
//! `ACK q TO seq` targets the shard embedded in `seq`'s shard bits: it
//! deletes every row with a local `__seq <= local(seq)` whose status is
//! `claimed`; rows still `ready` are not consumed by anyone and stay. A
//! poison message (`receive_count` past [`catalog::contract::queue::QUEUE_POISON_RECEIVE_COUNT`]) is
//! moved into the queue's dead-letter queue `<name>_dlq` instead of being
//! deleted. Everything runs in the statement's transaction, so the removals
//! take effect at commit and revert on rollback.
//!
//! `ACK q FOR <consumer> TO seq` is the fan-out path: it advances the
//! consumer cursor's frontier of that shard past `local(seq)` (monotonic,
//! never backwards) and then garbage-collects the shard rows every consumer
//! of the shard has acknowledged, in the same transaction. Fan-out rows are
//! never claimed or poisoned, so the GC deletes outright — no status check
//! and no DLQ interaction. The two modes are mutually exclusive per queue: a
//! queue with at least one consumer cursor rejects the default competing ack
//! with `InvalidArgument`, and an ack naming an unknown cursor fails with
//! `ObjectNotFound`.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use mudu::common::buf::Buf;
use mudu::common::result::RS;
use mudu::error::ErrorCode as ER;
use mudu::mudu_error;

use ::catalog::contract::cmd_exec::CmdExec;
use ::catalog::contract::meta_mgr::MetaMgr;
use ::catalog::contract::queue::{
    decode_queue_receive_count, decode_queue_status, queue_dlq_name, queue_seq_datum,
    queue_seq_local, QUEUE_PAYLOAD_ATTR_START, QUEUE_POISON_RECEIVE_COUNT,
    QUEUE_RECEIVE_COUNT_ATTR, QUEUE_SEQ_ATTR, QUEUE_STATUS_ATTR, QUEUE_STATUS_CLAIMED,
};
use ::catalog::contract::table_desc::TableDesc;
use ::xengine::api::{QueueShard, RangeData, VecDatum, VecSelTerm, XContract};
use ::xengine::x_param::PAckQueue;

/// Executor of a queue `ACK` statement.
pub struct AckQueue {
    param: PAckQueue,
    x_contract: Arc<dyn XContract>,
    meta_mgr: Arc<dyn MetaMgr>,
    affected_rows: AtomicU64,
}

impl AckQueue {
    /// The competing-cursor scan: deletes every claimed row of the shard at
    /// or below `seq` (poison messages move to the DLQ), returning
    /// `(acked, plain_deleted, plain_deleted_bytes)` — the plain counts
    /// exclude DLQ-transferred rows, whose deletion the transfer accounts
    /// for itself.
    async fn competing_ack_scan(
        &self,
        desc: &TableDesc,
        shard: &QueueShard,
        dlq: Option<&QueueShard>,
    ) -> RS<(u64, u64, u64)> {
        // Every column in attribute order; the payload columns are copied
        // verbatim when a poison message moves to the dead-letter queue.
        let select = VecSelTerm::new((0..desc.fields().len()).collect());
        let cursor = self
            .x_contract
            .read_range_on_partition(
                self.param.tx_mgr.clone(),
                self.param.table_id,
                shard.partition_id,
                &RangeData::new(
                    std::ops::Bound::Unbounded,
                    std::ops::Bound::Included(vec![(
                        QUEUE_SEQ_ATTR,
                        queue_seq_datum(self.param.seq)?,
                    )]),
                ),
                &select,
                &::xengine::api::OptRead::default(),
            )
            .await?;
        let mut acked = 0u64;
        let mut plain = 0u64;
        let mut plain_bytes = 0u64;
        while let Some(row) = cursor.next().await? {
            let fields = row.into_fields();
            let (Some(seq_bin), Some(status_bin)) = (
                fields[QUEUE_SEQ_ATTR].clone(),
                fields[QUEUE_STATUS_ATTR].clone(),
            ) else {
                continue;
            };
            if decode_queue_status(&status_bin)? != QUEUE_STATUS_CLAIMED {
                continue;
            }
            let receive_count = fields[QUEUE_RECEIVE_COUNT_ATTR]
                .as_ref()
                .map(|bin| decode_queue_receive_count(bin))
                .transpose()?
                .unwrap_or(0);
            let row_bytes = fields
                .iter()
                .skip(QUEUE_PAYLOAD_ATTR_START)
                .flatten()
                .map(|bin| bin.len() as u64)
                .sum::<u64>();
            if receive_count > QUEUE_POISON_RECEIVE_COUNT
                && self.move_to_dlq(shard, dlq, &fields).await?
            {
                acked += 1;
                continue;
            }
            let deleted = self
                .x_contract
                .delete_on_partition(
                    self.param.tx_mgr.clone(),
                    self.param.table_id,
                    shard.partition_id,
                    &VecDatum::new(vec![(QUEUE_SEQ_ATTR, seq_bin)]),
                )
                .await?;
            acked += deleted as u64;
            plain += deleted as u64;
            plain_bytes += row_bytes;
        }
        Ok((acked, plain, plain_bytes))
    }

    /// Creates an executor for the ack statement `param`, deleting and
    /// moving rows through `x_contract` and reading the catalog through
    /// `meta_mgr`.
    pub fn new(
        param: PAckQueue,
        x_contract: Arc<dyn XContract>,
        meta_mgr: Arc<dyn MetaMgr>,
    ) -> Self {
        Self {
            param,
            x_contract,
            meta_mgr,
            affected_rows: AtomicU64::new(0),
        }
    }

    /// The shard this statement acks (embedded in `seq`'s shard bits).
    async fn shard(&self) -> RS<QueueShard> {
        self.x_contract
            .queue_shard_of_seq(self.param.table_id, self.param.seq)
            .await
    }

    /// Resolves the single shard of the queue's dead-letter queue, when the
    /// DLQ exists.
    async fn dlq_shard(&self, desc: &TableDesc) -> RS<Option<QueueShard>> {
        let Some(dlq_desc) = self
            .meta_mgr
            .get_queue(desc.schema(), &queue_dlq_name(desc.name()))
            .await?
        else {
            return Ok(None);
        };
        let mut shards = self.x_contract.queue_shards(dlq_desc.id()).await?;
        if shards.len() != 1 {
            return Err(mudu_error!(
                ER::InvalidState,
                format!(
                    "dead-letter queue {} must not be partitioned",
                    dlq_desc.name()
                )
            ));
        }
        Ok(Some(shards.remove(0)))
    }
}

#[async_trait]
impl CmdExec for AckQueue {
    async fn prepare(&self) -> RS<()> {
        let desc = self.meta_mgr.get_table_by_id(self.param.table_id).await?;
        if !desc.is_queue() {
            return Err(mudu_error!(
                ER::ObjectNotFound,
                format!("no such queue {}", self.param.table_id)
            ));
        }
        let shard = self.shard().await?;
        match &self.param.consumer {
            Some(consumer) => {
                // The named cursor must exist; the default competing cursor
                // and the named cursors are mutually exclusive per queue.
                self.x_contract
                    .consumer_frontier_in_tx(self.param.tx_mgr.clone(), &shard, consumer)
                    .await?
                    .ok_or_else(|| {
                        mudu_error!(
                            ER::ObjectNotFound,
                            format!(
                                "no such consumer {consumer} on queue {}",
                                self.param.table_id
                            )
                        )
                    })?;
            }
            None => {
                if self
                    .x_contract
                    .consumers_exist_in_tx(self.param.tx_mgr.clone(), self.param.table_id)
                    .await?
                {
                    return Err(mudu_error!(
                        ER::InvalidArgument,
                        format!(
                            "queue {} has named consumers; use ACK ... FOR <consumer>",
                            self.param.table_id
                        )
                    ));
                }
            }
        }
        Ok(())
    }

    async fn run(&self) -> RS<()> {
        mudu_utils::scoped_task_trace!();
        let desc = self.meta_mgr.get_table_by_id(self.param.table_id).await?;
        let shard = self.shard().await?;
        let local_seq = queue_seq_local(self.param.seq);
        // Loads (and after a restart, rebuilds) the shard runtime state on
        // the owning worker.
        self.x_contract.queue_prepare_shard(&shard).await?;
        self.x_contract
            .queue_lock_shard(self.param.tx_mgr.clone(), &shard)
            .await?;
        if let Some(consumer) = &self.param.consumer {
            // The fan-out path: advance the cursor frontier and GC the
            // all-consumers-acknowledged prefix in the statement's
            // transaction. Returns the number of physically deleted rows.
            let deleted = self
                .x_contract
                .consumer_ack_and_gc(self.param.tx_mgr.clone(), &shard, consumer, local_seq)
                .await?;
            self.affected_rows.store(deleted, Ordering::Relaxed);
            return Ok(());
        }
        let dlq = self.dlq_shard(&desc).await?;
        let (acked, plain, plain_bytes) =
            self.competing_ack_scan(&desc, &shard, dlq.as_ref()).await?;
        // Restage the watermark with the advanced frontier and the exact
        // deleted counts. Rollback restores both messages and accounting.
        self.x_contract
            .queue_ack_in_tx(
                self.param.tx_mgr.clone(),
                &shard,
                local_seq,
                plain,
                plain_bytes,
            )
            .await?;
        self.affected_rows.store(acked, Ordering::Relaxed);
        Ok(())
    }

    async fn affected_rows(&self) -> RS<u64> {
        Ok(self.affected_rows.load(Ordering::Relaxed))
    }
}

impl AckQueue {
    /// Moves one poison message into the dead-letter queue: a fresh enqueue
    /// into the DLQ shard (own local `__seq` from the DLQ watermark, this
    /// transaction's xid as `__lsn`, reset status and receive count) with the
    /// payload columns copied verbatim, then the source row is deleted and
    /// the source watermark restaged. Returns `true` when the transfer
    /// happened (and restaged the source watermark); without a DLQ the row
    /// is deleted in place and the caller's accounting covers it (`false`).
    async fn move_to_dlq(
        &self,
        source: &QueueShard,
        dlq: Option<&QueueShard>,
        fields: &[Option<Buf>],
    ) -> RS<bool> {
        let Some(dlq) = dlq else {
            tracing::warn!(
                queue_oid = source.queue_oid,
                "poison message has no dead-letter queue; deleting it"
            );
            return Ok(false);
        };
        self.x_contract
            .queue_dlq_transfer(self.param.tx_mgr.clone(), source, dlq, fields)
            .await?;
        Ok(true)
    }
}
