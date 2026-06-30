use super::utils::{single_delete_batch, single_put_batch};
use super::*;

impl WorkerXContract {
    pub fn worker_begin_tx(&self) -> RS<Arc<dyn TxMgr>> {
        Ok(Arc::new(WorkerTxManager::new(
            self.snapshot_mgr.begin_tx()?,
        )))
    }

    pub fn worker_rollback_tx(&self, tx_mgr: Arc<dyn TxMgr>) -> RS<()> {
        self.snapshot_mgr.end_tx(tx_mgr.xid())
    }

    pub async fn worker_put_async(&self, key: Vec<u8>, value: Vec<u8>) -> RS<()> {
        let trace = task_trace!();
        trace.watch("put.stage", "contract_worker_put_start");
        let (storage, log, prepared) = {
            let xid = self.snapshot_mgr.alloc_committed_ts();
            trace.watch("put.xid", &xid.to_string());
            (
                self.storage.clone(),
                self.log_cloned()?,
                self.storage.prepare_worker_kv_autocommit(
                    xid,
                    key.clone(),
                    Some(value.clone()),
                    single_put_batch(xid, key, value),
                ),
            )
        };
        if let Some(log) = log {
            trace.watch("put.stage", "contract_worker_put_wal_append_start");
            new_xl_batch_writer(log).append(prepared.batch()).await?;
            trace.watch("put.stage", "contract_worker_put_wal_append_done");
        }
        trace.watch("put.stage", "contract_worker_put_storage_apply_start");
        storage.apply_prepared_commit_async(prepared).await
    }

    pub async fn worker_delete_async(&self, key: &[u8]) -> RS<()> {
        let key = key.to_vec();
        let (storage, log, prepared) = {
            let xid = self.snapshot_mgr.alloc_committed_ts();
            (
                self.storage.clone(),
                self.log_cloned()?,
                self.storage.prepare_worker_kv_autocommit(
                    xid,
                    key.clone(),
                    None,
                    single_delete_batch(xid, key),
                ),
            )
        };
        if let Some(log) = log {
            new_xl_batch_writer(log).append(prepared.batch()).await?;
        }
        storage.apply_prepared_commit_async(prepared).await
    }

    pub async fn worker_get_async(&self, key: &[u8]) -> RS<Option<Vec<u8>>> {
        self.storage.kv_get(key, None).await
    }

    pub async fn worker_get_with_snapshot_async(
        &self,
        snapshot: &WorkerSnapshot,
        key: &[u8],
    ) -> RS<Option<Vec<u8>>> {
        self.storage.kv_get(key, Some(snapshot)).await
    }

    pub async fn worker_range_scan_async(
        &self,
        start_key: &[u8],
        end_key: &[u8],
    ) -> RS<Vec<KvItem>> {
        self.storage.kv_range(start_key, end_key, None).await
    }

    pub async fn worker_range_scan_with_snapshot_async(
        &self,
        snapshot: &WorkerSnapshot,
        start_key: &[u8],
        end_key: &[u8],
    ) -> RS<Vec<KvItem>> {
        self.storage
            .kv_range(start_key, end_key, Some(snapshot))
            .await
    }

    pub fn log_cloned(&self) -> RS<Option<ChunkedWorkerLogBackend>> {
        let guard = self.log.lock()?;
        Ok(guard.clone())
    }
    pub async fn worker_commit_put_batch_async(
        &self,
        snapshot: &WorkerSnapshot,
        xid: u64,
        items: BTreeMap<Vec<u8>, Option<Vec<u8>>>,
        batch: XLBatch,
    ) -> RS<()> {
        if items.is_empty() {
            return self.snapshot_mgr.end_tx(xid);
        }
        let (storage, log, prepared) = {
            let prepared = self
                .storage
                .prepare_worker_kv_commit(snapshot, xid, items, batch)
                .await?;
            (self.storage.clone(), self.log_cloned()?, prepared)
        };
        if let Some(log) = log {
            new_xl_batch_writer(log.clone())
                .append(prepared.batch())
                .await?;
            log.flush_async().await?;
        }
        storage.apply_prepared_commit_async(prepared).await?;
        self.snapshot_mgr.end_tx(xid)
    }

    pub async fn worker_commit_tx_async(&self, tx: Arc<dyn TxMgr>) -> RS<()> {
        let _t = task_trace!();

        let xid = tx.xid();

        trace!("worker_commit_tx_async {}", xid);
        _t.watch("procedure.worker_commit.stage", "entry");
        _t.watch("procedure.worker_commit.xid", &xid.to_string());
        _t.watch("procedure.worker_commit.stage", "is_empty_check");
        if tx.is_empty() {
            _t.watch("procedure.worker_commit.stage", "rollback_empty_tx");
            return self.worker_rollback_tx(tx);
        }
        _t.watch("procedure.worker_commit.stage", "build_write_ops");
        tx.build_write_ops();
        let (storage, log, prepared) = {
            let write_ops = tx.write_ops();
            _t.watch("procedure.worker_commit.stage", "tx_lock_try_lock");
            let can_commit = self.tx_lock.try_lock_some(xid as OID, &write_ops)?;
            if !can_commit {
                _t.watch("procedure.worker_commit.stage", "tx_lock_failed");
                return Err(mudu_error!(
                    ErrorCode::Transaction,
                    format!("transaction {} failed to acquire commit locks", xid)
                ));
            }
            _t.watch("procedure.worker_commit.stage", "prepare_commit_start");
            let prepared = self.storage.prepare_commit_async(tx.as_ref()).await?;
            _t.watch("procedure.worker_commit.stage", "prepare_commit_done");
            (self.storage.clone(), self.log_cloned()?, prepared)
        };
        trace!("log flush {}", xid);
        let result = async {
            if let Some(log) = log {
                _t.watch("procedure.worker_execute.stage", "wal_append_start");
                new_xl_batch_writer(log.clone())
                    .append(prepared.batch())
                    .await?;
                _t.watch("procedure.worker_execute.stage", "wal_append_done");
                _t.watch("procedure.worker_execute.stage", "wal_flush_start");
                log.flush_async().await?;
                _t.watch("procedure.worker_execute.stage", "wal_flush_done");
            }
            _t.watch("procedure.worker_execute.stage", "storage_apply_start");
            storage.apply_prepared_commit_async(prepared).await?;
            _t.watch("procedure.worker_execute.stage", "storage_apply_done");
            Ok(())
        }
        .await;
        trace!("log flush done {}", xid);
        let write_ops = tx.write_ops();
        _t.watch("procedure.worker_commit.stage", "tx_lock_release");
        self.tx_lock.release(xid as OID, &write_ops)?;
        _t.watch("procedure.worker_commit.stage", "rollback_tx_cleanup");
        self.worker_rollback_tx(tx)?;
        _t.watch("procedure.worker_commit.stage", "done");
        trace!("worker_commit_tx_async finish {}", xid);
        result
    }

    pub async fn replay_worker_log_batch(&self, batch: XLBatch) -> RS<()> {
        let max_xid = batch.entries.iter().map(|entry| entry.xid).max();
        if let Some(max_xid) = max_xid {
            self.snapshot_mgr.observe_committed_ts(max_xid);
        }
        self.storage.replay_batch(batch).await
    }

    pub fn finish_worker_log_recovery(&self) -> RS<()> {
        Ok(())
    }

    pub async fn recover_pending_cross_partition_records_async(&self) -> RS<()> {
        Ok(())
    }

    pub fn ensure_partition_rpc_handler(self: &Arc<Self>) -> RS<()> {
        if self.partition_rpc_registered.swap(true, Ordering::SeqCst) {
            return Ok(());
        }
        debug!(
            worker_id = self.worker_id,
            "registering partition rpc handler"
        );
        let bus = current_message_bus()?;
        let contract = self.clone();
        bus.on_recv_callback(
            RecvFilter {
                dst: Some(self.worker_id),
                kind: Some(PARTITION_RPC_REQUEST_KIND),
                ..RecvFilter::default()
            },
            Arc::new(move |envelope| {
                let contract = contract.clone();
                Box::pin(async move { contract.handle_partition_rpc(envelope).await })
            }),
        )?;
        Ok(())
    }
}
