use super::*;

impl WorkerXContract {
    /// Replay one WAL batch into storage during recovery, advancing the
    /// snapshot watermark past the batch's maximum xid.
    pub async fn replay_worker_log_batch(&self, batch: XLBatch) -> RS<()> {
        let max_xid = batch.entries.iter().map(|entry| entry.xid).max();
        if let Some(max_xid) = max_xid {
            self.snapshot_mgr.observe_committed_ts(max_xid);
        }
        self.storage.replay_batch(batch).await
    }

    /// Mark the end of WAL recovery (a no-op: recovery is fully driven by
    /// [`Self::replay_worker_log_batch`]).
    pub fn finish_worker_log_recovery(&self) -> RS<()> {
        Ok(())
    }

    /// Recover cross-partition transactions left pending by a crash (a
    /// no-op: in-doubt cross-partition transactions are resolved by their
    /// coordinators).
    pub async fn recover_pending_cross_partition_records_async(&self) -> RS<()> {
        Ok(())
    }
}
