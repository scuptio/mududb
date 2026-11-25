use super::*;
use crate::contract::WorkerXContract;
use crate::utils::statement_lock_token;
use crate::worker_tx_manager::WorkerTxManager;
use crate::x_lock_mgr::XLockMgr;

/// Begin a new transaction on this worker's snapshot manager.
pub fn worker_begin_tx(snapshot_mgr: &WorkerSnapshotMgr) -> RS<Arc<dyn TxMgr>> {
    Ok(Arc::new(WorkerTxManager::new(snapshot_mgr.begin_tx()?)))
}

/// Begins a transaction reading at the current epoch (not the stable
/// one): for control-plane re-homing under an exclusive partition
/// fence (phase 3 splits/merges), where live-edge-committed versions
/// are provably committed.
pub(crate) fn worker_begin_tx_reading_current(
    snapshot_mgr: &WorkerSnapshotMgr,
) -> RS<Arc<dyn TxMgr>> {
    Ok(Arc::new(WorkerTxManager::new(
        snapshot_mgr.begin_tx_reading_current()?,
    )))
}

/// Begins a statement-shell transaction with no epoch registration (see
/// [`WorkerSnapshotMgr::begin_tx_untracked`]): autocommit statements
/// whose work runs in autonomous inner transactions (queue
/// `RECEIVE ... WAIT`) must not pin the collecting epoch while parked.
pub fn worker_begin_tx_untracked(snapshot_mgr: &WorkerSnapshotMgr) -> RS<Arc<dyn TxMgr>> {
    Ok(Arc::new(WorkerTxManager::new(
        snapshot_mgr.begin_tx_untracked()?,
    )))
}

/// Rolls back the transaction's local state (releases its statement
/// locks and its epoch registration). Called by the worker's session
/// cleanup when a session closes with an open transaction.
pub fn worker_rollback_tx(
    tx_lock: &XLockMgr,
    snapshot_mgr: &WorkerSnapshotMgr,
    tx_mgr: Arc<dyn TxMgr>,
) -> RS<()> {
    // Drop every statement-level lock this transaction took locally (a
    // no-op for transactions that never locked).
    tx_lock.release_all(tx_mgr.xid() as OID)?;
    snapshot_mgr.end_tx(tx_mgr.xid())
}

/// The node epoch oracle's current epoch, when an oracle drives
/// commits. Captured by the session/procedure commit paths just BEFORE
/// the commit (while the transaction's registration still pins the
/// local oracle's live edge, the capture equals the transaction's
/// joined epoch — and every inner transaction of the statement joined
/// the same epoch; the cluster oracle's ticker-driven capture may be
/// newer, which is conservative and safe).
pub fn worker_current_epoch(
    snapshot_mgr: &WorkerSnapshotMgr,
) -> Option<catalog::contract::epoch_oracle::CommitEpoch> {
    snapshot_mgr
        .epoch_oracle()
        .map(|oracle| oracle.current_epoch())
}

/// Wait until `epoch` is stable: the read-your-writes gate the
/// session/procedure BEGIN paths run before registering a fresh
/// transaction (design doc §5.2 — the session carries its commit
/// watermark and waits it out at begin, so commits never block on the
/// barrier and the wait never deadlocks: the waiter holds no
/// registration of its own yet).
pub async fn worker_wait_epoch_stable(
    snapshot_mgr: &WorkerSnapshotMgr,
    epoch: catalog::contract::epoch_oracle::CommitEpoch,
) -> RS<()> {
    if let Some(oracle) = snapshot_mgr.epoch_oracle() {
        oracle.wait_stable(epoch).await?;
    }
    Ok(())
}

impl WorkerXContract {
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
        worker_rollback_tx(&self.tx_lock, &self.snapshot_mgr, tx)
    }
}
