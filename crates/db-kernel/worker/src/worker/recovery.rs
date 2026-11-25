//! Worker WAL replay and transaction recovery entry points.
use super::*;

impl WorkerRuntime {
    /// Replay one WAL batch into storage during recovery, advancing the
    /// snapshot watermark past the batch's maximum xid.
    pub async fn replay_log_batch(&self, batch: XLBatch) -> RS<()> {
        self.contract.replay_worker_log_batch(batch).await
    }

    /// Replays a complete serial batch with its verified exclusive WAL end.
    /// This certifies the business cut for snapshots, not merely append durability.
    pub async fn replay_log_batch_at(&self, batch: XLBatch, wal_resume: u64) -> RS<()> {
        self.contract
            .replay_worker_log_batch_at(batch, wal_resume)
            .await
    }

    /// Mark the end of WAL recovery after the last replayed batch.
    pub fn finish_log_recovery(&self) -> RS<()> {
        self.contract.finish_worker_log_recovery()
    }

    /// Recover cross-partition transactions left pending by a crash.
    pub async fn recover_cross_partition_transactions(&self) -> RS<()> {
        self.contract
            .recover_pending_cross_partition_records_async()
            .await
    }
}
