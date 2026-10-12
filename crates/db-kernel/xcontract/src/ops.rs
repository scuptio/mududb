use super::*;

mod delete;
mod insert;
mod locks;
mod read;
mod update;

impl WorkerXContract {
    pub(crate) fn _begin_tx(&self) -> RS<Arc<dyn TxMgr>> {
        Ok(Arc::new(WorkerTxManager::new(
            self.snapshot_mgr.begin_tx()?,
        )))
    }
}
