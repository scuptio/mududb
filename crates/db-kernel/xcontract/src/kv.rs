use super::*;

mod commit;
mod kv_ops;
mod recovery;
mod rpc_register;
mod tx;

impl WorkerXContract {
    pub(crate) fn log_cloned(&self) -> RS<Option<ChunkedWorkerLogBackend>> {
        let guard = self.log.lock()?;
        Ok(guard.clone())
    }
}
