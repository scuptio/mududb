use super::*;
use crate::wal::lsn::LSN;
use crate::wal::typed_worker_log::WorkerLogRecoveryHandler;
use crate::wal::worker_log::{ChunkedWorkerLogBackend, WorkerLogBackend, WorkerLogRecoverySource};
use crate::wal::xl_batch::XLBatch;
use async_trait::async_trait;
use std::path::{Path, PathBuf};

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

struct WorkerRingLoopRecoverySource {
    backend: ChunkedWorkerLogBackend,
}

unsafe impl Send for WorkerRingLoopRecoverySource {}
unsafe impl Sync for WorkerRingLoopRecoverySource {}

#[async_trait]
impl WorkerLogRecoverySource for WorkerRingLoopRecoverySource {
    async fn chunk_paths_sorted(&self) -> RS<Vec<PathBuf>> {
        self.backend.chunk_paths_sorted().await
    }

    async fn read_chunk(&self, path: &Path) -> RS<Vec<u8>> {
        Ok(self.backend.fs().read_all(path).await?)
    }
}

impl WorkerRingLoop {
    /// Replays persisted worker-log chunks before the worker starts serving
    /// live traffic.
    pub(super) fn recover_worker_log_on_loop(&mut self) -> RS<()> {
        let log = match self.log.take() {
            Some(log) => log,
            None => return Ok(()),
        };
        let worker_id = self.worker.worker_id();
        trace!(worker_id, "worker_ring_loop recover_worker_log start");
        let backend = log.backend().clone();
        let recovery = async move {
            let mut source = WorkerRingLoopRecoverySource { backend };
            let result = log.recover(&mut source).await;
            Ok((log, result))
        };
        let (log, result) = self.drive_local_future(recovery, "worker log recovery")?;
        self.log = Some(log);
        trace!(
            worker_id,
            ok = result.is_ok(),
            "worker_ring_loop recover_worker_log finished"
        );
        result
    }
}
