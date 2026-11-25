use super::*;
use crate::worker_tx_manager::WorkerTxManager;

mod delete;
mod insert;
pub(crate) mod locks;
mod read;
mod update;

/// Starts an untracked transaction over the worker's snapshot manager.
pub(crate) fn _begin_tx(snapshot_mgr: &WorkerSnapshotMgr) -> RS<Arc<dyn TxMgr>> {
    Ok(Arc::new(WorkerTxManager::new(snapshot_mgr.begin_tx()?)))
}
