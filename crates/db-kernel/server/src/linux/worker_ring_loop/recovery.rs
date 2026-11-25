use super::*;
use ::wal::lsn::LSN;
use ::wal::typed_worker_log::WorkerLogRecoveryHandler;
use ::wal::xl_batch::XLBatch;
use async_trait::async_trait;

pub(super) struct WorkerRingLoopRecoveryHandler {
    pub(super) worker: WorkerRuntime,
}

#[async_trait]
impl WorkerLogRecoveryHandler<XLBatch> for WorkerRingLoopRecoveryHandler {
    async fn handle_entry(&self, entry: XLBatch, _start_lsn: LSN) -> RS<()> {
        self.worker.replay_log_batch(entry).await
    }

    fn finish(&self) -> RS<()> {
        self.worker.finish_log_recovery()
    }
}

impl WorkerRingLoop {
    /// Replays the durable snapshot base and WAL suffix before serving.
    pub(super) fn recover_worker_log_on_loop(&mut self) -> RS<()> {
        let worker = self.worker.clone();
        self.drive_local_future(
            async move { ::repl::snapshot::recover_worker_storage(&worker).await },
            "worker snapshot and WAL recovery",
        )
    }
}
