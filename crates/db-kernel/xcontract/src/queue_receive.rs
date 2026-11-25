//! Owner-side `RECEIVE` execution of queue shards: claim rounds, the WAIT
//! loops, consumer fan-out scans, and the poison-message DLQ transfer.
//!
//! Every routine here runs on whichever worker coordinates the statement —
//! shard reads/writes go through the partition-pinned operators
//! (`read_range_pinned`, `update_pinned`, ...), which execute locally on the
//! shard owner and through the partition RPC against a remote owner. The
//! `RECEIVE ... WAIT` loops are the exception: their claim rounds commit in
//! autonomous transactions *on the shard owner* and the park registers on the
//! owner's wait registry, so a remote caller dispatches the whole loop with
//! the `QueueReceiveWait` partition RPC.
use super::*;
use crate::contract::WorkerXContract;
use crate::ops::_begin_tx;
use crate::queue::resolve_queue_shards;
use crate::queue_types::ensure_local_shard_owner;
use crate::queue_types::shard_key;
use ::catalog::contract::cluster_router::ClusterRouter;
use ::catalog::contract::queue::{
    decode_queue_receive_count, decode_queue_seq, decode_queue_status, queue_dlq_name,
    queue_lsn_datum, queue_receive_count_datum, queue_seq_datum, queue_seq_encode, queue_seq_local,
    queue_status_datum, QueueWait, QUEUE_LSN_ATTR, QUEUE_PAYLOAD_ATTR_START,
    QUEUE_POISON_RECEIVE_COUNT, QUEUE_RECEIVE_COUNT_ATTR, QUEUE_SEQ_ATTR, QUEUE_SEQ_LOCAL_SENTINEL,
    QUEUE_STATUS_ATTR, QUEUE_STATUS_CLAIMED, QUEUE_STATUS_READY,
};
use ::xengine::api::DeltaOp;
use mudu_sys::time::instant_now;

/// One claimed message of a receive round.
pub(crate) struct ClaimedRow {
    /// Global (shard-bit encoded) `__seq` of the row.
    pub seq: u64,
    /// `receive_count` after this claim.
    pub receive_count: i32,
    /// Full attribute vector of the row as scanned (pre-claim values).
    pub fields: Vec<Option<Buf>>,
}

/// Resolves the (single) shard of the dead-letter queue; the DLQ must not
/// be partitioned.
async fn resolve_dlq_shard(
    meta_mgr: &Arc<dyn MetaMgr>,
    worker_id: OID,
    cluster_router: &Option<Arc<dyn ClusterRouter>>,
    default_unpartitioned_worker_id: OID,
    dlq_desc: &Arc<TableDesc>,
) -> RS<QueueShard> {
    let mut shards = resolve_queue_shards(
        meta_mgr,
        worker_id,
        cluster_router,
        default_unpartitioned_worker_id,
        dlq_desc.id(),
    )
    .await?;
    if shards.len() != 1 {
        return Err(mudu_error!(
            ErrorCode::InvalidState,
            format!(
                "dead-letter queue {} must not be partitioned",
                dlq_desc.name()
            )
        ));
    }
    Ok(shards.remove(0))
}

impl WorkerXContract {
    /// Moves one poison message of `source` into the queue's dead-letter
    /// queue `dlq`: a fresh enqueue into the DLQ shard (own local sequence,
    /// this transaction's xid as `__lsn`, reset status and receive count)
    /// with the payload columns copied verbatim, then the delete of the
    /// source row and the source shard's watermark restage. Everything rides
    /// the caller's transaction.
    pub(crate) async fn queue_dlq_transfer_async(
        &self,
        tx: Arc<dyn TxMgr>,
        source: &QueueShard,
        dlq: &QueueShard,
        fields: &[Option<Buf>],
    ) -> RS<()> {
        let dlq_desc = self.meta_mgr.get_table_by_id(dlq.queue_oid).await?;
        let bytes = fields
            .iter()
            .skip(QUEUE_PAYLOAD_ATTR_START)
            .flatten()
            .map(|bin| bin.len() as u64)
            .sum();
        let local_seq = self.reserve_queue(tx.clone(), dlq, 1, bytes).await?;
        let seq = queue_seq_encode(dlq.shard_index, local_seq)?;
        let key = VecDatum::new(vec![(QUEUE_SEQ_ATTR, queue_seq_datum(seq)?)]);
        let mut value = vec![
            (QUEUE_LSN_ATTR, queue_lsn_datum(tx.xid())?),
            (QUEUE_STATUS_ATTR, queue_status_datum(QUEUE_STATUS_READY)?),
            (QUEUE_RECEIVE_COUNT_ATTR, queue_receive_count_datum(0)?),
        ];
        value.extend(
            fields
                .iter()
                .enumerate()
                .skip(QUEUE_PAYLOAD_ATTR_START)
                .filter_map(|(attr, bin)| bin.as_ref().map(|bin| (attr, bin.clone()))),
        );
        self.insert_pinned(
            dlq_desc,
            tx.clone(),
            dlq.queue_oid,
            Some(dlq.partition_id),
            &key,
            &VecDatum::new(value),
        )
        .await?;
        let Some(seq_bin) = &fields[QUEUE_SEQ_ATTR] else {
            return Ok(());
        };
        let source_seq = decode_queue_seq(seq_bin)?;
        self.delete_on_partition(
            tx.clone(),
            source.queue_oid,
            source.partition_id,
            &VecDatum::new(vec![(QUEUE_SEQ_ATTR, seq_bin.clone())]),
        )
        .await?;
        self.ack_queue_state(tx, source, queue_seq_local(source_seq), 1, bytes)
            .await
    }

    /// One competing-cursor claim round inside the caller's transaction:
    /// scans the shard in local `__seq` order and claims ready rows until
    /// `count` is reached. Poison rows (receive count at or past the
    /// threshold) are claimed and returned among the rest; the caller moves
    /// them to the DLQ through [`WorkerXContract::queue_dlq_transfer_async`].
    pub(crate) async fn claim_round_in_tx(
        &self,
        tx: Arc<dyn TxMgr>,
        shard: &QueueShard,
        count: u64,
    ) -> RS<Vec<ClaimedRow>> {
        self.lock_shard_state(tx.clone(), shard).await?;
        let desc = self.meta_mgr.get_table_by_id(shard.queue_oid).await?;
        // The scan projects every column in attribute order; `status`/
        // `receive_count`/`__state_*` stay internal to the claim logic.
        let select = VecSelTerm::new((0..desc.fields().len()).collect());
        let cursor = self
            .read_range_pinned(
                desc.clone(),
                tx.clone(),
                shard.queue_oid,
                shard.partition_id,
                &RangeData::new(Bound::Unbounded, Bound::Unbounded),
                &Predicate::CNF(Vec::new()),
                &select,
            )
            .await?;
        let mut claimed: Vec<ClaimedRow> = Vec::new();
        while (claimed.len() as u64) < count {
            let Some(row) = cursor.next().await? else {
                break;
            };
            let fields = row.into_fields();
            let (Some(_seq_bin), Some(status_bin)) = (
                fields[QUEUE_SEQ_ATTR].clone(),
                fields[QUEUE_STATUS_ATTR].clone(),
            ) else {
                continue;
            };
            let status = decode_queue_status(&status_bin)?;
            if status != QUEUE_STATUS_READY {
                // Claimed rows and the watermark sentinel are skipped.
                continue;
            }
            if let Some(row) = self.claim_one(tx.clone(), shard, &desc, fields).await? {
                claimed.push(row);
            }
        }
        Ok(claimed)
    }

    /// Claims one ready row: `status = claimed` as an absolute assignment and
    /// `receive_count + 1` as a statement-time delta evaluated against the
    /// latest committed row under the statement lock (locally or on the
    /// owner). `None` when the row vanished or is locked by an in-flight
    /// transaction. The returned row carries the PRE-claim scanned values —
    /// the poison check compares against the receive count observed before
    /// this claim, so a message is dead-lettered only once its count passes
    /// the threshold before yet another delivery.
    async fn claim_one(
        &self,
        tx: Arc<dyn TxMgr>,
        shard: &QueueShard,
        desc: &Arc<TableDesc>,
        fields: Vec<Option<Buf>>,
    ) -> RS<Option<ClaimedRow>> {
        let Some(seq_bin) = fields[QUEUE_SEQ_ATTR].clone() else {
            return Ok(None);
        };
        let pre_claim_count = fields[QUEUE_RECEIVE_COUNT_ATTR]
            .as_ref()
            .map(|bin| decode_queue_receive_count(bin))
            .transpose()?
            .unwrap_or(0);
        let key = VecDatum::new(vec![(QUEUE_SEQ_ATTR, seq_bin.clone())]);
        let values = VecDatum::new(vec![(
            QUEUE_STATUS_ATTR,
            queue_status_datum(QUEUE_STATUS_CLAIMED)?,
        )]);
        let deltas = [DeltaAssign {
            attr: QUEUE_RECEIVE_COUNT_ATTR,
            op: DeltaOp::Add,
            literal: queue_receive_count_datum(1)?,
        }];
        let updated = match self
            .update_pinned(
                desc.clone(),
                tx,
                shard.queue_oid,
                Some(shard.partition_id),
                &key,
                &values,
                &deltas,
                &[],
            )
            .await
        {
            Ok(1) => 1,
            Ok(_) => 0,
            // A lock-timeout conflict means a concurrent in-flight
            // transaction claimed the row; skip it and continue with the next
            // ready message.
            Err(err) if err.ec() == ErrorCode::Transaction => 0,
            Err(err) => return Err(err),
        };
        if updated == 0 {
            return Ok(None);
        }
        Ok(Some(ClaimedRow {
            seq: decode_queue_seq(&seq_bin)?,
            receive_count: pre_claim_count,
            fields,
        }))
    }

    /// One fan-out scan round for a consumer cursor: appends the full field
    /// vectors of rows with local `__seq >= from_local` (in `__seq` order)
    /// until `count`; returns the local sequence the next round continues
    /// from. The scan never claims and skips the watermark sentinel, so rows
    /// stay `ready` and visible to every other cursor.
    pub(crate) async fn consumer_scan_round_in_tx(
        &self,
        tx: Arc<dyn TxMgr>,
        shard: &QueueShard,
        from_local: u64,
        count: u64,
    ) -> RS<(Vec<Vec<Option<Buf>>>, u64)> {
        let desc = self.meta_mgr.get_table_by_id(shard.queue_oid).await?;
        let select = VecSelTerm::new((0..desc.fields().len()).collect());
        let from_seq = queue_seq_encode(shard.shard_index, from_local)?;
        let cursor = self
            .read_range_pinned(
                desc,
                tx,
                shard.queue_oid,
                shard.partition_id,
                &RangeData::new(
                    Bound::Included(vec![(QUEUE_SEQ_ATTR, queue_seq_datum(from_seq)?)]),
                    Bound::Unbounded,
                ),
                &Predicate::CNF(Vec::new()),
                &select,
            )
            .await?;
        let mut rows = Vec::new();
        let mut next_from = from_local;
        while (rows.len() as u64) < count {
            let Some(row) = cursor.next().await? else {
                break;
            };
            let fields = row.into_fields();
            let Some(seq_bin) = fields[QUEUE_SEQ_ATTR].clone() else {
                continue;
            };
            let seq = decode_queue_seq(&seq_bin)?;
            if queue_seq_local(seq) == QUEUE_SEQ_LOCAL_SENTINEL {
                continue;
            }
            next_from = queue_seq_local(seq).saturating_add(1);
            rows.push(fields);
        }
        Ok((rows, next_from))
    }

    /// The `RECEIVE ... WAIT` entry point: local shards run the loop
    /// in-process; remote shards dispatch the loop to the owner with the
    /// `QueueReceiveWait` partition RPC.
    pub(crate) async fn receive_wait(
        &self,
        shard: &QueueShard,
        count: u64,
        wait: QueueWait,
        consumer: Option<String>,
    ) -> RS<Vec<Vec<Option<Buf>>>> {
        if shard.worker_id == self.worker_id {
            self.receive_wait_loop(shard, count, wait, consumer).await
        } else {
            self.remote_queue_receive_wait(shard, count, wait, consumer)
                .await
        }
    }

    /// The owner-side WAIT loop: scan/claim rounds run in short transactions
    /// that commit before parking, so the park itself holds no transaction or
    /// locks. The waiter registers before the first scan and clears its
    /// sticky signal before every round; a commit notification therefore
    /// either applies before the upcoming scan (the scan sees the new
    /// messages) or re-arms the latch for the next round — no wakeup is lost.
    ///
    /// Runs on the shard owner (the wait registry is owner-local).
    pub(crate) async fn receive_wait_loop(
        &self,
        shard: &QueueShard,
        count: u64,
        wait: QueueWait,
        consumer: Option<String>,
    ) -> RS<Vec<Vec<Option<Buf>>>> {
        ensure_local_shard_owner(shard, self.worker_id)?;
        let deadline = match wait {
            QueueWait::Timeout(ms) => Some(instant_now() + Duration::from_millis(ms)),
            QueueWait::Forever => None,
        };
        let waiter = self.queue_wait_registry.register(shard_key(shard))?;
        let mut rows: Vec<Vec<Option<Buf>>> = Vec::new();
        let mut scan_from = match &consumer {
            Some(consumer) => self
                .consumer_frontier(shard, consumer)
                .await?
                .ok_or_else(|| {
                    mudu_error!(
                        ErrorCode::ObjectNotFound,
                        format!("no such consumer {consumer} on queue {}", shard.queue_oid)
                    )
                })?,
            None => 0,
        };
        loop {
            waiter.clear_signal();
            self.wait_round_with_retries(shard, &consumer, count, &mut rows, &mut scan_from)
                .await?;
            if !rows.is_empty() || count == 0 {
                return Ok(rows);
            }
            let remaining = match deadline {
                Some(deadline) => {
                    let now = instant_now();
                    if now >= deadline {
                        return Ok(rows);
                    }
                    Some(deadline - now)
                }
                None => None,
            };
            waiter.park(remaining).await;
        }
    }

    /// One scan/claim round with bounded SI-conflict retries: conflicts
    /// against a concurrent round's commit are normal under epoch-coarse
    /// stamping — abort the round and retry it with a fresh snapshot
    /// (bounded; a persistent conflict surfaces).
    async fn wait_round_with_retries(
        &self,
        shard: &QueueShard,
        consumer: &Option<String>,
        count: u64,
        rows: &mut Vec<Vec<Option<Buf>>>,
        scan_from: &mut u64,
    ) -> RS<()> {
        /// Bound on SI-conflict retries of one claim round.
        const MAX_ROUND_RETRIES: u32 = 8;
        let rows_before = rows.len();
        let scan_from_before = *scan_from;
        let mut round_retries = 0u32;
        loop {
            let round_tx = _begin_tx(&self.snapshot_mgr)?;
            let result = match consumer {
                Some(consumer) => {
                    self.consumer_wait_round(
                        round_tx.clone(),
                        shard,
                        consumer,
                        scan_from,
                        rows,
                        count,
                    )
                    .await
                }
                None => {
                    self.claim_wait_round(round_tx.clone(), shard, rows, count)
                        .await
                }
            };
            match result {
                Ok(()) => match self.worker_commit_routed_tx_async(round_tx.clone()).await {
                    Ok(_) => return Ok(()),
                    Err(err)
                        if err.ec() == ErrorCode::Transaction
                            && round_retries < MAX_ROUND_RETRIES =>
                    {
                        round_retries += 1;
                        let _ = self.worker_abort_tx_async(round_tx).await;
                        rows.truncate(rows_before);
                        *scan_from = scan_from_before;
                        continue;
                    }
                    Err(err) => {
                        let _ = self.worker_abort_tx_async(round_tx).await;
                        return Err(err);
                    }
                },
                Err(err) => {
                    let _ = self.worker_abort_tx_async(round_tx).await;
                    return Err(err);
                }
            }
        }
    }

    /// One competing-cursor WAIT round: claims up to `count` ready rows in
    /// the round transaction; poison rows move to the DLQ instead of being
    /// delivered.
    async fn claim_wait_round(
        &self,
        round_tx: Arc<dyn TxMgr>,
        shard: &QueueShard,
        rows: &mut Vec<Vec<Option<Buf>>>,
        count: u64,
    ) -> RS<()> {
        self.ensure_shard_runtime_loaded(shard).await?;
        let desc = self.meta_mgr.get_table_by_id(shard.queue_oid).await?;
        let dlq = self
            .meta_mgr
            .get_queue(desc.schema(), &queue_dlq_name(desc.name()))
            .await?;
        let remaining = count.saturating_sub(rows.len() as u64);
        let claimed = self
            .claim_round_in_tx(round_tx.clone(), shard, remaining)
            .await?;
        for row in claimed {
            if row.receive_count >= QUEUE_POISON_RECEIVE_COUNT {
                if let Some(dlq_desc) = &dlq {
                    let dlq_shard = resolve_dlq_shard(
                        &self.meta_mgr,
                        self.worker_id,
                        &self.cluster_router,
                        self.default_unpartitioned_worker_id,
                        dlq_desc,
                    )
                    .await?;
                    self.queue_dlq_transfer_async(round_tx.clone(), shard, &dlq_shard, &row.fields)
                        .await?;
                    continue;
                }
            }
            rows.push(row.fields);
        }
        Ok(())
    }

    /// One fan-out WAIT round: read-only scan from the cursor frontier (the
    /// round transaction stages nothing, so committing it just ends it).
    /// Re-validates the cursor every round so a dropped cursor surfaces as
    /// `ObjectNotFound` instead of an endless rescan.
    async fn consumer_wait_round(
        &self,
        round_tx: Arc<dyn TxMgr>,
        shard: &QueueShard,
        consumer: &str,
        scan_from: &mut u64,
        rows: &mut Vec<Vec<Option<Buf>>>,
        count: u64,
    ) -> RS<()> {
        self.ensure_consumer_state_loaded(shard.queue_oid, shard.shard_index)
            .await?;
        let frontier = self
            .consumer_frontier_in_tx(round_tx.clone(), shard, consumer)
            .await?
            .ok_or_else(|| {
                mudu_error!(
                    ErrorCode::ObjectNotFound,
                    format!("no such consumer {consumer} on queue {}", shard.queue_oid)
                )
            })?;
        let base = frontier.max(*scan_from);
        let remaining = count.saturating_sub(rows.len() as u64);
        let (mut round_rows, next_from) = self
            .consumer_scan_round_in_tx(round_tx, shard, base, remaining)
            .await?;
        *scan_from = next_from;
        rows.append(&mut round_rows);
        Ok(())
    }
}
