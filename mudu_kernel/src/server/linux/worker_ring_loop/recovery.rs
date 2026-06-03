use super::*;
use crate::wal::lsn::LSN;
use crate::wal::typed_worker_log::WorkerLogRecoveryHandler;
use crate::wal::worker_log::{ChunkedWorkerLogBackend, WorkerLogBackend, WorkerLogRecoverySource};
use crate::wal::xl_batch::XLBatch;
use std::path::{Path, PathBuf};
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

struct WorkerRingLoopRecoverySource {
    loop_ref: *mut WorkerRingLoop,
    backend: ChunkedWorkerLogBackend,
}

unsafe impl Send for WorkerRingLoopRecoverySource {}
unsafe impl Sync for WorkerRingLoopRecoverySource {}

#[async_trait]
impl WorkerLogRecoverySource for WorkerRingLoopRecoverySource {
    async fn chunk_paths_sorted(& self) -> RS<Vec<PathBuf>> {
        self.backend.chunk_paths_sorted().await
    }

    async fn read_chunk(&self, path: &Path) -> RS<Vec<u8>> {
        let file = OpenOptions::new()
            .read(true)
            .open(path)
            .map_err(|e| m_error!(EC::IOErr, "open worker log chunk for recovery error", e))?;
        let size = file
            .metadata()
            .map_err(|e| {
                m_error!(
                    EC::IOErr,
                    "read worker log chunk recovery metadata error",
                    e
                )
            })?
            .len() as usize;
        if size == 0 {
            return Ok(Vec::new());
        }
        unsafe { (&mut *self.loop_ref).read_file_all_iouring(&file, size) }
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
        trace!(
            worker_id,
            "worker_ring_loop recover_worker_log start"
        );
        let backend = log.backend().clone();
        let loop_ref = self as *mut WorkerRingLoop;
        let recovery = async move {
            let mut source = WorkerRingLoopRecoverySource { loop_ref, backend };
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

    /// Reads a full file through this loop's io_uring instance.
    ///
    /// Recovery uses this helper to keep all I/O on the same ring that the
    /// worker will later use for live operations.
    fn read_file_all_iouring(&mut self, file: &std::fs::File, size: usize) -> RS<Vec<u8>> {
        let mut buf = vec![0u8; size];
        let mut offset = 0usize;
        while offset < size {
            let Some(mut sqe) = self.ring.next_sqe() else {
                let submitted = self.ring.submit();
                if submitted < 0 {
                    return Err(m_error!(
                        EC::IOErr,
                        format!("submit io_uring recovery read error {}", submitted)
                    ));
                }
                continue;
            };
            sqe.set_user_data(0);
            sqe.prep_read_raw(
                file.as_raw_fd(),
                buf[offset..].as_mut_ptr(),
                size - offset,
                offset as u64,
            );
            let submitted = self.ring.submit();
            if submitted < 0 {
                return Err(m_error!(
                    EC::IOErr,
                    format!("submit io_uring recovery read error {}", submitted)
                ));
            }
            let read = match self.ring.wait() {
                Ok(cqe) => cqe.result(),
                Err(wait_rc) => {
                    return Err(m_error!(
                        EC::IOErr,
                        format!("wait io_uring recovery read cqe error {}", wait_rc)
                    ))
                }
            };
            if read < 0 {
                return Err(m_error!(
                    EC::IOErr,
                    format!("worker log recovery read completion error {}", read)
                ));
            }
            if read == 0 {
                break;
            }
            offset += read as usize;
        }
        buf.truncate(offset);
        Ok(buf)
    }
}
