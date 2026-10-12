use super::*;
use crate::utils::{statement_lock_token, STATEMENT_LOCK_TIMEOUT};

impl WorkerXContract {
    /// Take a statement-level pessimistic write lock on `key` of
    /// `relation_id` on the local worker (waiting, bounded by
    /// `STATEMENT_LOCK_TIMEOUT`) and record it on the transaction for
    /// commit/rollback release. Re-entrant for keys the transaction already
    /// holds.
    pub(super) async fn acquire_statement_lock(
        &self,
        tx_mgr: &dyn TxMgr,
        relation_id: PhysicalRelationId,
        key: Vec<u8>,
    ) -> RS<()> {
        let xid = tx_mgr.xid();
        let acquired = {
            let _stage =
                ::catalog::stage_stats::StageGuard::new(::catalog::stage_stats::Stage::StmtLock);
            self.tx_lock
                .lock_some(
                    xid as OID,
                    &[(relation_id, key.clone())],
                    STATEMENT_LOCK_TIMEOUT,
                )
                .await?
        };
        if !acquired {
            return Err(mudu_error!(
                ErrorCode::Transaction,
                format!("transaction {} failed to acquire statement locks", xid)
            ));
        }
        tx_mgr.record_statement_lock(relation_id, key);
        Ok(())
    }

    /// Take a statement-level write lock on a remote-owned key via
    /// `LockKeyForUpdate` (lock + read in one round trip) and record the
    /// owner for rollback release. Returns the currently committed value,
    /// projected like `remote_read_key`.
    pub(super) async fn lock_remote_key_for_update(
        &self,
        tx_mgr: &dyn TxMgr,
        worker_id: OID,
        table_id: OID,
        partition_id: OID,
        key: Vec<u8>,
        select: Vec<AttrIndex>,
    ) -> RS<Option<Vec<Option<DataBin>>>> {
        let lock_token = statement_lock_token(self.worker_id, tx_mgr.xid());
        let value = self
            .remote_lock_key_for_update(worker_id, lock_token, table_id, partition_id, key, select)
            .await?;
        tx_mgr.record_remote_lock_owner(worker_id);
        Ok(value)
    }
}
