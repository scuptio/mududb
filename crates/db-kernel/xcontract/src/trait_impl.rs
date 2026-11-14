use super::*;
use ::catalog::contract::queue::QueueWait;

#[async_trait]
impl XContract for WorkerXContract {
    async fn create_table(&self, _tx_mgr: Arc<dyn TxMgr>, schema: &SchemaTable) -> RS<()> {
        self.storage.create_table_async(schema).await
    }

    async fn drop_table(&self, _tx_mgr: Arc<dyn TxMgr>, oid: OID) -> RS<()> {
        self.storage.drop_table_async(oid).await
    }

    async fn create_queue(&self, _tx_mgr: Arc<dyn TxMgr>, schema: &SchemaTable) -> RS<()> {
        // Runtime counters register when the command prepares each shard
        // after the catalog writes (the binding lands after the queue row in
        // the catalog, so shard resolution is only possible then).
        self.storage.create_queue_async(schema).await
    }

    async fn drop_queue(&self, _tx_mgr: Arc<dyn TxMgr>, oid: OID) -> RS<()> {
        self.storage.drop_queue_async(oid).await?;
        self.queue_registry.unregister_queue(oid)?;
        // Wake parked waiters so a `WAIT FOREVER` re-scans, observes the drop,
        // and fails instead of hanging on a queue that no longer exists.
        self.queue_wait_registry.remove_queue(oid)?;
        // Stop the queue's activation driver, if any (activations live on
        // unpartitioned queues only).
        self.activation_registry.unregister(oid)?;
        // Delete the queue's `_queue_consumer` rows in a best-effort internal
        // transaction (DDL is non-transactional, like the rest of this chain)
        // and evict the in-memory cursor state.
        self.drop_queue_consumers_async(oid).await?;
        self.consumer_registry.unregister_queue(oid)?;
        Ok(())
    }

    async fn queue_shards(&self, table_id: OID) -> RS<Vec<QueueShard>> {
        self.resolve_queue_shards(table_id).await
    }

    async fn queue_shard_of_seq(&self, table_id: OID, seq: u64) -> RS<QueueShard> {
        self.resolve_queue_shard_of_seq(table_id, seq).await
    }

    async fn queue_route_payload(&self, table_id: OID, payload: &VecDatum) -> RS<QueueShard> {
        self.route_payload_shard(table_id, payload).await
    }

    async fn queue_receive_shard(&self, table_id: OID) -> RS<QueueShard> {
        let shards = self.resolve_queue_shards(table_id).await?;
        let index = self
            .queue_registry
            .rotate_receive_shard(table_id, shards.len() as u64)?;
        shards
            .into_iter()
            .find(|shard| shard.shard_index == index)
            .ok_or_else(|| {
                mudu_error!(
                    ErrorCode::InvalidState,
                    format!("queue {table_id} has no shard {index}")
                )
            })
    }

    async fn queue_prepare_shard(&self, shard: &QueueShard) -> RS<()> {
        if shard.worker_id == self.worker_id {
            self.ensure_shard_runtime_loaded(shard).await?;
            // The named consumer cursors ride the same lazy load: the first
            // shard access after a restart also rebuilds the fan-out cursor
            // cache from the committed `_queue_consumer` rows.
            self.ensure_consumer_state_loaded(shard.queue_oid, shard.shard_index)
                .await
        } else {
            self.remote_queue_prepare(shard).await
        }
    }

    async fn queue_reserve(
        &self,
        tx: Arc<dyn TxMgr>,
        shard: &QueueShard,
        count: u64,
        bytes: u64,
    ) -> RS<u64> {
        self.reserve_queue(tx, shard, count, bytes).await
    }

    async fn queue_lock_shard(&self, tx: Arc<dyn TxMgr>, shard: &QueueShard) -> RS<()> {
        self.lock_shard_state(tx, shard).await
    }

    async fn queue_ack_in_tx(
        &self,
        tx: Arc<dyn TxMgr>,
        shard: &QueueShard,
        local_seq: u64,
        deleted: u64,
        deleted_bytes: u64,
    ) -> RS<()> {
        self.ack_queue_state(tx, shard, local_seq, deleted, deleted_bytes)
            .await
    }

    async fn insert_on_partition(
        &self,
        tx: Arc<dyn TxMgr>,
        table_id: OID,
        partition_id: OID,
        key: &VecDatum,
        value: &VecDatum,
    ) -> RS<()> {
        let desc = self.meta_mgr.get_table_by_id(table_id).await?;
        self.insert_pinned(desc, tx, table_id, Some(partition_id), key, value)
            .await
    }

    async fn update_on_partition(
        &self,
        tx: Arc<dyn TxMgr>,
        table_id: OID,
        partition_id: OID,
        key: &VecDatum,
        values: &VecDatum,
        opt: &OptUpdate,
    ) -> RS<usize> {
        let desc = self.meta_mgr.get_table_by_id(table_id).await?;
        self.update_pinned(
            desc,
            tx,
            table_id,
            Some(partition_id),
            key,
            values,
            &opt.delta_assignments,
            &opt.null_assignments,
        )
        .await
    }

    async fn delete_on_partition(
        &self,
        tx: Arc<dyn TxMgr>,
        table_id: OID,
        partition_id: OID,
        key: &VecDatum,
    ) -> RS<usize> {
        let desc = self.meta_mgr.get_table_by_id(table_id).await?;
        self.delete_pinned(desc, tx, table_id, Some(partition_id), key)
            .await
    }

    async fn read_range_on_partition(
        &self,
        tx: Arc<dyn TxMgr>,
        table_id: OID,
        partition_id: OID,
        range: &RangeData,
        select: &VecSelTerm,
        _opt: &OptRead,
    ) -> RS<Arc<dyn RSCursor>> {
        let desc = self.meta_mgr.get_table_by_id(table_id).await?;
        self.read_range_pinned(
            desc,
            tx,
            table_id,
            partition_id,
            range,
            &Predicate::CNF(Vec::new()),
            select,
        )
        .await
    }

    async fn queue_receive_wait(
        &self,
        shard: &QueueShard,
        count: u64,
        wait: QueueWait,
        consumer: Option<String>,
    ) -> RS<Vec<Vec<Option<DataBin>>>> {
        self.receive_wait(shard, count, wait, consumer).await
    }

    async fn queue_claim_round(
        &self,
        tx: Arc<dyn TxMgr>,
        shard: &QueueShard,
        count: u64,
    ) -> RS<Vec<QueueClaimedRow>> {
        let claimed = self.claim_round_in_tx(tx, shard, count).await?;
        Ok(claimed
            .into_iter()
            .map(|row| QueueClaimedRow {
                seq: row.seq,
                receive_count: row.receive_count,
                fields: row.fields,
            })
            .collect())
    }

    async fn queue_consumer_scan(
        &self,
        tx: Arc<dyn TxMgr>,
        shard: &QueueShard,
        consumer: &str,
        count: u64,
    ) -> RS<Vec<Vec<Option<DataBin>>>> {
        let frontier = self
            .consumer_frontier_in_tx(tx.clone(), shard, consumer)
            .await?
            .ok_or_else(|| {
                mudu_error!(
                    ErrorCode::EntityNotFound,
                    format!("no such consumer {consumer} on queue {}", shard.queue_oid)
                )
            })?;
        let (rows, _) = self
            .consumer_scan_round_in_tx(tx, shard, frontier, count)
            .await?;
        Ok(rows)
    }

    async fn queue_dlq_transfer(
        &self,
        tx: Arc<dyn TxMgr>,
        source: &QueueShard,
        dlq: &QueueShard,
        fields: &[Option<DataBin>],
    ) -> RS<()> {
        self.queue_dlq_transfer_async(tx, source, dlq, fields).await
    }

    fn queue_register_waiter(&self, shard: &QueueShard) -> RS<Arc<dyn QueueWaiter>> {
        Ok(self.queue_wait_registry.register(shard_key(shard))?)
    }

    async fn consumer_list_in_tx(
        &self,
        tx: Arc<dyn TxMgr>,
        queue_oid: OID,
    ) -> RS<Vec<QueueConsumerEntry>> {
        self.consumer_scan_in_tx(tx, queue_oid).await
    }

    async fn consumer_frontier_in_tx(
        &self,
        tx: Arc<dyn TxMgr>,
        shard: &QueueShard,
        consumer: &str,
    ) -> RS<Option<u64>> {
        Ok(self
            .consumer_scan_in_tx(tx, shard.queue_oid)
            .await?
            .into_iter()
            .find(|entry| entry.shard == shard.shard_index && entry.name == consumer)
            .map(|entry| entry.frontier))
    }

    async fn consumers_exist_in_tx(&self, tx: Arc<dyn TxMgr>, queue_oid: OID) -> RS<bool> {
        Ok(!self.consumer_scan_in_tx(tx, queue_oid).await?.is_empty())
    }

    async fn consumer_create(
        &self,
        tx_mgr: Arc<dyn TxMgr>,
        queue_oid: OID,
        consumer: &str,
    ) -> RS<()> {
        self.consumer_create_async(tx_mgr, queue_oid, consumer)
            .await
    }

    async fn consumer_drop(
        &self,
        tx_mgr: Arc<dyn TxMgr>,
        queue_oid: OID,
        consumer: &str,
    ) -> RS<()> {
        self.consumer_drop_async(tx_mgr, queue_oid, consumer).await
    }

    /// Lists the cursor cache entries of the shards hosted on this worker;
    /// the management plane merges these across the shard owners.
    async fn consumer_list(&self, queue_oid: OID) -> RS<Vec<QueueConsumerEntry>> {
        let shards = self.resolve_queue_shards(queue_oid).await?;
        let mut entries = Vec::new();
        for shard in shards {
            if shard.worker_id != self.worker_id {
                continue;
            }
            self.ensure_consumer_state_loaded(queue_oid, shard.shard_index)
                .await?;
            for (name, frontier) in self.consumer_registry.list(queue_oid, shard.shard_index)? {
                entries.push(QueueConsumerEntry {
                    shard: shard.shard_index,
                    name,
                    frontier,
                });
            }
        }
        entries.sort_by(|a, b| a.name.cmp(&b.name).then(a.shard.cmp(&b.shard)));
        Ok(entries)
    }

    async fn consumer_frontier(&self, shard: &QueueShard, consumer: &str) -> RS<Option<u64>> {
        self.ensure_consumer_state_loaded(shard.queue_oid, shard.shard_index)
            .await?;
        self.consumer_registry
            .frontier(shard.queue_oid, shard.shard_index, consumer)
    }

    async fn consumers_exist(&self, queue_oid: OID) -> RS<bool> {
        let tx = self._begin_tx()?;
        let result = self.consumer_scan_in_tx(tx.clone(), queue_oid).await;
        let _ = self.worker_rollback_tx(tx);
        Ok(!result?.is_empty())
    }

    async fn consumer_ack_and_gc(
        &self,
        tx_mgr: Arc<dyn TxMgr>,
        shard: &QueueShard,
        consumer: &str,
        seq: u64,
    ) -> RS<u64> {
        self.consumer_ack_and_gc_async(tx_mgr, shard, consumer, seq)
            .await
    }

    async fn alter_table(
        &self,
        _tx_mgr: Arc<dyn TxMgr>,
        _oid: OID,
        _alter_table: &AlterTable,
    ) -> RS<()> {
        Err(mudu_error!(
            ErrorCode::NotImplemented,
            "alter table is not implemented"
        ))
    }

    async fn begin_tx(&self) -> RS<Arc<dyn TxMgr>> {
        self._begin_tx()
    }

    async fn commit_tx(&self, tx_mgr: Arc<dyn TxMgr>) -> RS<()> {
        self.worker_commit_routed_tx_async(tx_mgr).await
    }

    async fn abort_tx(&self, tx_mgr: Arc<dyn TxMgr>) -> RS<()> {
        self.worker_abort_tx_async(tx_mgr).await
    }

    async fn update(
        &self,
        tx_mgr: Arc<dyn TxMgr>,
        table_id: OID,
        pred_key: &VecDatum,
        pred_non_key: &Predicate,
        values: &VecDatum,
        opt_update: &OptUpdate,
    ) -> RS<usize> {
        let desc = self.meta_mgr.get_table_by_id(table_id).await?;
        self._update(
            desc,
            tx_mgr,
            table_id,
            pred_key,
            pred_non_key,
            values,
            &opt_update.delta_assignments,
            &opt_update.null_assignments,
        )
        .await
    }

    async fn read_key(
        &self,
        tx_mgr: Arc<dyn TxMgr>,
        table_id: OID,
        pred_key: &VecDatum,
        select: &VecSelTerm,
        _opt_read: &OptRead,
    ) -> RS<Option<Vec<Option<DataBin>>>> {
        let desc = self.meta_mgr.get_table_by_id(table_id).await?;
        self._read_key(desc, tx_mgr, table_id, pred_key, select)
            .await
    }

    async fn read_range(
        &self,
        tx_mgr: Arc<dyn TxMgr>,
        table_id: OID,
        pred_key: &RangeData,
        pred_non_key: &Predicate,
        select: &VecSelTerm,
        _opt_read: &OptRead,
    ) -> RS<Arc<dyn RSCursor>> {
        let desc = self.meta_mgr.get_table_by_id(table_id).await?;
        self._read_range(desc, tx_mgr, table_id, pred_key, pred_non_key, select)
            .await
    }

    async fn delete(
        &self,
        tx_mgr: Arc<dyn TxMgr>,
        table_id: OID,
        pred_key: &VecDatum,
        pred_non_key: &Predicate,
        opt_delete: &OptDelete,
    ) -> RS<usize> {
        let desc = self.meta_mgr.get_table_by_id(table_id).await?;
        self._delete(desc, tx_mgr, table_id, pred_key, pred_non_key, opt_delete)
            .await
    }

    async fn insert(
        &self,
        tx_mgr: Arc<dyn TxMgr>,
        table_id: OID,
        keys: &VecDatum,
        values: &VecDatum,
        opt_insert: &OptInsert,
    ) -> RS<()> {
        scoped_task_trace!();
        let desc = self.meta_mgr.get_table_by_id(table_id).await?;
        self._insert(desc, tx_mgr, table_id, keys, values, opt_insert)
            .await
    }

    fn local_worker_id(&self) -> OID {
        self.worker_id()
    }
}
