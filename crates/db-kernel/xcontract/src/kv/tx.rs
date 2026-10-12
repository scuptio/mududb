use super::*;
use crate::utils::statement_lock_token;

impl WorkerXContract {
    /// Begin a new transaction on this worker's snapshot manager.
    pub fn worker_begin_tx(&self) -> RS<Arc<dyn TxMgr>> {
        Ok(Arc::new(WorkerTxManager::new(
            self.snapshot_mgr.begin_tx()?,
        )))
    }

    pub(crate) fn worker_rollback_tx(&self, tx_mgr: Arc<dyn TxMgr>) -> RS<()> {
        // Drop every statement-level lock this transaction took locally (a
        // no-op for transactions that never locked).
        self.tx_lock.release_all(tx_mgr.xid() as OID)?;
        self.snapshot_mgr.end_tx(tx_mgr.xid())
    }

    /// Roll back `tx`, additionally releasing any statement-level locks it
    /// holds on remote owner workers (best-effort; orphan reclamation on the
    /// owner is the backstop).
    pub async fn worker_abort_tx_async(&self, tx: Arc<dyn TxMgr>) -> RS<()> {
        let owners = tx.remote_lock_owners();
        if !owners.is_empty() {
            let token = statement_lock_token(self.worker_id, tx.xid());
            for owner in owners {
                if let Err(err) = self.remote_unlock_keys(owner, token).await {
                    debug!(
                        worker_id = self.worker_id,
                        owner, "remote unlock keys on abort failed: {err}"
                    );
                }
            }
        }
        self.worker_rollback_tx(tx)
    }
}
