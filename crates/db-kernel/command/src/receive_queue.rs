//! Executor of a queue `RECEIVE` statement: claims up to `count` ready
//! messages of one shard, or scans them for a named consumer cursor.
//!
//! Shard selection: a sharded queue is received one shard per statement —
//! the shard rotates round-robin over the queue's shards, so repeated
//! RECEIVE statements drain every shard (the per-shard activation driver is
//! the primary consumer anyway). An unpartitioned queue always resolves to
//! its single shard. `__seq` embeds the shard bits, so `ACK` finds the shard
//! of every received message without extra bookkeeping.
//!
//! Without a WAIT clause the claim runs in the statement's transaction: every
//! claimed row is updated in place (`status` ready -> claimed, `receive_count
//! + 1`) under its statement lock — the claim takes effect at commit and
//! reverts to `ready` on rollback. Rows claimed by an in-flight concurrent
//! transaction surface as statement-lock conflicts and are skipped; rows
//! already claimed and committed read as `claimed` and are skipped without
//! touching the lock manager.
//!
//! With WAIT the claim rounds run owner-side in short autonomous transactions
//! committing before parking on the shard's wait registry: the park holds no
//! transaction, snapshot, or locks. A commit that writes the shard wakes the
//! head waiter, which re-scans.
//!
//! With `FOR <consumer>` (fan-out mode) the statement is read-only: it scans
//! the rows at or past the consumer cursor's frontier without claiming them.
//! Nothing is claimed or updated, so every cursor sees every message.
use std::collections::VecDeque;
use std::sync::Arc;

use mudu::common::buf::Buf;
use mudu::common::result::RS;
use mudu_contract::tuple::tuple_field::TupleField;
use mudu_contract::tuple::tuple_field_desc::TupleFieldDesc;
use mudu_sys::sync::async_::futures_mutex::FMutex;

use ::catalog::contract::meta_mgr::MetaMgr;
use ::catalog::contract::query_exec::QueryExec;
use ::catalog::contract::queue::{
    queue_dlq_name, QUEUE_LSN_ATTR, QUEUE_PAYLOAD_ATTR_START, QUEUE_POISON_RECEIVE_COUNT,
    QUEUE_SEQ_ATTR,
};
use ::xengine::api::{QueueShard, XContract};
use ::xengine::x_param::PReceiveQueue;

/// Executor of a queue `RECEIVE` statement.
pub struct ReceiveQueue {
    tuple_desc: TupleFieldDesc,
    inner: FMutex<_InnerReceiveQueue>,
}

struct _InnerReceiveQueue {
    param: PReceiveQueue,
    x_contract: Arc<dyn XContract>,
    meta_mgr: Arc<dyn MetaMgr>,
    rows: VecDeque<TupleField>,
}

impl ReceiveQueue {
    /// Creates an executor for the receive statement `param`, claiming
    /// through `x_contract` and reading the catalog through `meta_mgr`.
    pub fn new(
        param: PReceiveQueue,
        x_contract: Arc<dyn XContract>,
        meta_mgr: Arc<dyn MetaMgr>,
    ) -> Self {
        let tuple_desc = param.tuple_desc.clone();
        Self {
            tuple_desc,
            inner: FMutex::new(_InnerReceiveQueue {
                param,
                x_contract,
                meta_mgr,
                rows: VecDeque::new(),
            }),
        }
    }
}

#[async_trait::async_trait]
impl QueryExec for ReceiveQueue {
    async fn open(&self) -> RS<()> {
        let mut inner = self.inner.lock().await;
        inner.open().await
    }

    async fn next(&self) -> RS<Option<TupleField>> {
        let mut inner = self.inner.lock().await;
        Ok(inner.rows.pop_front())
    }

    fn tuple_desc(&self) -> RS<TupleFieldDesc> {
        Ok(self.tuple_desc.clone())
    }
}

impl _InnerReceiveQueue {
    async fn open(&mut self) -> RS<()> {
        let shard = self
            .x_contract
            .queue_receive_shard(self.param.table_id)
            .await?;
        self.x_contract.queue_prepare_shard(&shard).await?;
        match self.param.consumer.clone() {
            Some(consumer) => self.open_for_consumer(&shard, &consumer).await,
            None => self.open_competing(&shard).await,
        }
    }

    /// The competing-cursor open: claims land in the statement transaction
    /// and commit or roll back with it; WAIT goes through the owner-side
    /// loop.
    async fn open_competing(&mut self, shard: &QueueShard) -> RS<()> {
        // The default competing cursor and the named consumer cursors are
        // mutually exclusive per queue.
        if self
            .x_contract
            .consumers_exist_in_tx(self.param.tx_mgr.clone(), self.param.table_id)
            .await?
        {
            return Err(mudu::mudu_error!(
                mudu::error::ErrorCode::InvalidArgument,
                format!(
                    "queue {} has named consumers; use RECEIVE ... FOR <consumer>",
                    self.param.table_id
                )
            ));
        }
        match self.param.wait {
            None => {
                let claimed = self
                    .x_contract
                    .queue_claim_round(self.param.tx_mgr.clone(), shard, self.param.count)
                    .await?;
                for row in claimed {
                    if row.receive_count >= QUEUE_POISON_RECEIVE_COUNT {
                        if let Some(dlq) = self.dlq_shard(shard).await? {
                            self.x_contract
                                .queue_dlq_transfer(
                                    self.param.tx_mgr.clone(),
                                    shard,
                                    &dlq,
                                    &row.fields,
                                )
                                .await?;
                            continue;
                        }
                    }
                    self.rows.push_back(envelope_row(row.fields));
                }
            }
            Some(wait) => {
                let rows = self
                    .x_contract
                    .queue_receive_wait(shard, self.param.count, wait, None)
                    .await?;
                self.rows.extend(rows.into_iter().map(envelope_row));
            }
        }
        Ok(())
    }

    /// The fan-out open: a read-only scan of the shard rows at or past the
    /// consumer cursor's frontier. The scan never claims, so the rows stay
    /// `ready` and visible to every other cursor.
    async fn open_for_consumer(&mut self, shard: &QueueShard, consumer: &str) -> RS<()> {
        match self.param.wait {
            None => {
                let rows = self
                    .x_contract
                    .queue_consumer_scan(
                        self.param.tx_mgr.clone(),
                        shard,
                        consumer,
                        self.param.count,
                    )
                    .await?;
                self.rows.extend(rows.into_iter().map(envelope_row));
            }
            Some(wait) => {
                let rows = self
                    .x_contract
                    .queue_receive_wait(shard, self.param.count, wait, Some(consumer.to_string()))
                    .await?;
                self.rows.extend(rows.into_iter().map(envelope_row));
            }
        }
        Ok(())
    }

    /// Resolves the shard of the queue's dead-letter queue, when the DLQ
    /// exists.
    async fn dlq_shard(&self, shard: &QueueShard) -> RS<Option<QueueShard>> {
        let desc = self.meta_mgr.get_table_by_id(self.param.table_id).await?;
        let Some(dlq_desc) = self
            .meta_mgr
            .get_queue(desc.schema(), &queue_dlq_name(desc.name()))
            .await?
        else {
            return Ok(None);
        };
        let mut shards = self.x_contract.queue_shards(dlq_desc.id()).await?;
        if shards.len() != 1 {
            return Err(mudu::mudu_error!(
                mudu::error::ErrorCode::InvalidState,
                format!(
                    "dead-letter queue {} must not be partitioned",
                    dlq_desc.name()
                )
            ));
        }
        let _ = shard;
        Ok(Some(shards.remove(0)))
    }
}

/// Assembles the envelope row (`__seq`, `__lsn`, payload columns) from a
/// scanned row holding every column in attribute order.
fn envelope_row(fields: Vec<Option<Buf>>) -> TupleField {
    let mut envelope = Vec::with_capacity(fields.len() - (QUEUE_PAYLOAD_ATTR_START - 2));
    envelope.push(fields[QUEUE_SEQ_ATTR].clone());
    envelope.push(fields[QUEUE_LSN_ATTR].clone());
    envelope.extend_from_slice(&fields[QUEUE_PAYLOAD_ATTR_START..]);
    TupleField::new_nullable(envelope)
}
