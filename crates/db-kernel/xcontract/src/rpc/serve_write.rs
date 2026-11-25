//! Write-path handlers of the partition RPC service: single-row
//! insert/delete/update through the normal local commit path, the
//! single-owner `CommitWriteSet` handoff commit, and the statement-level
//! key locks (`LockKeyForUpdate` / `UnlockKeys`) backing it.

use super::*;
use crate::kv::tx::{worker_begin_tx, worker_begin_tx_reading_current, worker_rollback_tx};
use crate::utils::{apply_value_update, project_selected_fields, STATEMENT_LOCK_TIMEOUT};
use crate::worker_storage::WorkerStorage;
use crate::x_lock_mgr::XLockMgr;
use ::catalog::contract::meta_mgr::MetaMgr;
use ::catalog::contract::worker_snapshot::WorkerSnapshotMgr;

/// Releases every statement lock the caller's `lock_token` holds on this
/// worker.
async fn execute_rpc_unlock_keys(
    tx_lock: &XLockMgr,
    worker_id: OID,
    lock_token: OID,
) -> RS<PartitionRpcResponse> {
    debug!(worker_id, "execute partition rpc unlock_keys");
    tx_lock.release_all(lock_token)?;
    Ok(PartitionRpcResponse::UnlockKeys)
}

/// Serve `LockKeyForUpdate`: lock `key` for the caller's `lock_token`,
/// then read and project the currently committed value.
#[allow(clippy::too_many_arguments)]
async fn execute_rpc_lock_key_for_update(
    tx_lock: &XLockMgr,
    meta_mgr: &Arc<dyn MetaMgr>,
    storage: &Arc<WorkerStorage>,
    snapshot_mgr: &WorkerSnapshotMgr,
    worker_id: OID,
    lock_token: OID,
    table_id: OID,
    partition_id: OID,
    key: Vec<u8>,
    select: Vec<AttrIndex>,
) -> RS<PartitionRpcResponse> {
    debug!(
        worker_id,
        table_id,
        partition_id,
        key_len = key.len(),
        "execute partition rpc lock_key_for_update"
    );
    let relation_id = PhysicalRelationId {
        table_id,
        partition_id,
    };
    let acquired = tx_lock
        .lock_some(
            lock_token,
            &[(relation_id, key.clone())],
            STATEMENT_LOCK_TIMEOUT,
        )
        .await?;
    if !acquired {
        return Err(mudu_error!(
            ErrorCode::Transaction,
            "failed to acquire statement locks"
        ));
    }
    // Locked: read the currently committed value with a fresh
    // snapshot (same as ReadKey).
    let desc = meta_mgr.get_table_by_id(table_id).await?;
    let tx_mgr = worker_begin_tx(snapshot_mgr)?;
    let opt_value = storage
        .get_on_partition(table_id, Some(partition_id), &key, tx_mgr.as_ref())
        .await?;
    worker_rollback_tx(tx_lock, snapshot_mgr, tx_mgr)?;
    let projected = opt_value
        .map(|value| project_selected_fields(&desc, &key, &value, &VecSelTerm::new(select)))
        .transpose()?;
    Ok(PartitionRpcResponse::ReadKey(projected))
}

impl WorkerXContract {
    /// Dispatches one write-path RPC of the partition service.
    pub(super) async fn execute_write_rpc(
        &self,
        request: PartitionRpcRequest,
    ) -> RS<PartitionRpcResponse> {
        match request {
            PartitionRpcRequest::Insert {
                table_id,
                partition_id,
                key,
                value,
            } => {
                self.execute_rpc_insert(table_id, partition_id, key, value)
                    .await
            }
            PartitionRpcRequest::Delete {
                table_id,
                partition_id,
                key,
            } => self.execute_rpc_delete(table_id, partition_id, key).await,
            PartitionRpcRequest::Update {
                table_id,
                partition_id,
                key,
                values,
            } => {
                self.execute_rpc_update(table_id, partition_id, key, values)
                    .await
            }
            PartitionRpcRequest::CommitWriteSet {
                tx_id,
                lock_token,
                writes,
            } => {
                self.execute_rpc_commit_write_set(tx_id, lock_token, writes)
                    .await
            }
            PartitionRpcRequest::LockKeyForUpdate {
                lock_token,
                table_id,
                partition_id,
                key,
                select,
            } => {
                execute_rpc_lock_key_for_update(
                    &self.tx_lock,
                    &self.meta_mgr,
                    &self.storage,
                    &self.snapshot_mgr,
                    self.worker_id,
                    lock_token,
                    table_id,
                    partition_id,
                    key,
                    select,
                )
                .await
            }
            PartitionRpcRequest::UnlockKeys { lock_token } => {
                execute_rpc_unlock_keys(&self.tx_lock, self.worker_id, lock_token).await
            }
            _ => Err(mudu_error!(
                ErrorCode::Internal,
                "not a write-path partition rpc"
            )),
        }
    }

    /// Serve `Insert`: fail when the key exists, otherwise stage and commit
    /// the row through the normal local commit path.
    async fn execute_rpc_insert(
        &self,
        table_id: OID,
        partition_id: OID,
        key: Vec<u8>,
        value: Vec<u8>,
    ) -> RS<PartitionRpcResponse> {
        debug!(
            worker_id = self.worker_id,
            table_id,
            partition_id,
            key_len = key.len(),
            value_len = value.len(),
            "execute partition rpc insert"
        );
        let tx_mgr = worker_begin_tx(&self.snapshot_mgr)?;
        let current = self
            .storage
            .get_on_partition(table_id, Some(partition_id), &key, tx_mgr.as_ref())
            .await?;
        if current.is_some() {
            worker_rollback_tx(&self.tx_lock, &self.snapshot_mgr, tx_mgr)?;
            return Err(mudu_error!(ErrorCode::ObjectAlreadyExists, "existing key"));
        }
        self.storage
            .put_on_partition(table_id, Some(partition_id), key, value, tx_mgr.as_ref())
            .await?;
        let _ = self.worker_commit_tx_async(tx_mgr).await?;
        Ok(PartitionRpcResponse::Insert)
    }

    /// Serve `Delete`: remove the key through the normal local commit path;
    /// the response carries the number of removed rows.
    async fn execute_rpc_delete(
        &self,
        table_id: OID,
        partition_id: OID,
        key: Vec<u8>,
    ) -> RS<PartitionRpcResponse> {
        debug!(
            worker_id = self.worker_id,
            table_id,
            partition_id,
            key_len = key.len(),
            "execute partition rpc delete"
        );
        let tx_mgr = worker_begin_tx(&self.snapshot_mgr)?;
        let deleted = self
            .storage
            .remove_on_partition(table_id, Some(partition_id), &key, tx_mgr.as_ref())
            .await?;
        let _ = self.worker_commit_tx_async(tx_mgr).await?;
        Ok(PartitionRpcResponse::Delete(usize::from(deleted.is_some())))
    }

    /// Serve `Update`: replace the given columns of `key`'s row through the
    /// normal local commit path; the response carries the number of changed
    /// rows.
    async fn execute_rpc_update(
        &self,
        table_id: OID,
        partition_id: OID,
        key: Vec<u8>,
        values: Vec<(AttrIndex, DataBin)>,
    ) -> RS<PartitionRpcResponse> {
        debug!(
            worker_id = self.worker_id,
            table_id,
            partition_id,
            key_len = key.len(),
            value_pairs = values.len(),
            "execute partition rpc update"
        );
        let desc = self.meta_mgr.get_table_by_id(table_id).await?;
        let tx_mgr = worker_begin_tx(&self.snapshot_mgr)?;
        let current = self
            .storage
            .get_on_partition(table_id, Some(partition_id), &key, tx_mgr.as_ref())
            .await?;
        let Some(current) = current else {
            worker_rollback_tx(&self.tx_lock, &self.snapshot_mgr, tx_mgr)?;
            return Ok(PartitionRpcResponse::Update(0));
        };
        let updated = apply_value_update(&current, &VecDatum::new(values), &[], &desc)?;
        self.storage
            .put_on_partition(table_id, Some(partition_id), key, updated, tx_mgr.as_ref())
            .await?;
        let _ = self.worker_commit_tx_async(tx_mgr).await?;
        Ok(PartitionRpcResponse::Update(1))
    }

    /// Serve `CommitWriteSet`: stage the handed-off write set into a fresh
    /// local transaction and commit it, retrying a bounded number of times
    /// on commit-time transaction failures.
    async fn execute_rpc_commit_write_set(
        &self,
        tx_id: OID,
        lock_token: Option<OID>,
        writes: Vec<XLWrite>,
    ) -> RS<PartitionRpcResponse> {
        debug!(
            worker_id = self.worker_id,
            coordinator_tx_id = tx_id,
            writes = writes.len(),
            "execute partition rpc commit_write_set"
        );
        let write_count = writes.len();
        // The write set is fully materialized (final values) and each
        // attempt commits it with a fresh owner-local snapshot, so a
        // commit-time Transaction failure (commit-lock contention or
        // a write-write conflict against a commit that landed inside
        // this RPC's window) is retried a bounded number of times.
        // Retrying does not weaken isolation: the coordinator's
        // snapshot is deliberately not propagated to the owner.
        const MAX_COMMIT_WRITE_SET_ATTEMPTS: u32 = 2;
        let mut attempt = 0;
        let result = loop {
            attempt += 1;
            match self.commit_write_set_once(&writes, lock_token).await {
                Ok(()) => break Ok(()),
                Err(err)
                    if err.ec() == ErrorCode::Transaction
                        && attempt < MAX_COMMIT_WRITE_SET_ATTEMPTS =>
                {
                    debug!(
                        worker_id = self.worker_id,
                        coordinator_tx_id = tx_id,
                        attempt,
                        "commit_write_set retrying after transaction failure"
                    );
                }
                Err(err) => break Err(err),
            }
        };
        // The commit path releases `lock_token`'s locks on every
        // exit; this is the backstop for attempts that failed before
        // reaching it.
        if let Some(token) = lock_token {
            self.tx_lock.release_all(token)?;
        }
        result?;
        Ok(PartitionRpcResponse::CommitWriteSet(write_count))
    }

    /// Stage a handed-off write set into a fresh local transaction and commit
    /// it through the normal local commit path. When `lock_token` is present
    /// the commit runs under the coordinator's statement locks (re-entrant
    /// acquisition) and they are all released when the commit returns.
    ///
    /// The fresh transaction reads at the live edge (`begin_tx_reading_current`):
    /// the coordinator's snapshot is deliberately not propagated to the owner
    /// (isolation rests on the statement locks and the coordinator-side
    /// validation), and every version present in storage is committed — the
    /// pre-epoch build behaved the same way because a fresh xid snapshot
    /// observed every committed version. Reading the stable epoch instead
    /// would turn any same-epoch commit the statement itself triggered
    /// (e.g. a queue claim reset) into a phantom write-write conflict.
    async fn commit_write_set_once(&self, writes: &[XLWrite], lock_token: Option<OID>) -> RS<()> {
        let tx_mgr = worker_begin_tx_reading_current(&self.snapshot_mgr)?;
        let lock_owner = lock_token.unwrap_or(tx_mgr.xid() as OID);
        for write in writes {
            match write {
                XLWrite::Insert(insert) => tx_mgr.put_relation(
                    PhysicalRelationId {
                        table_id: insert.table_id,
                        partition_id: insert.partition_id,
                    },
                    insert.key.clone(),
                    insert.value.clone(),
                ),
                XLWrite::Delete(delete) => tx_mgr.delete_relation(
                    PhysicalRelationId {
                        table_id: delete.table_id,
                        partition_id: delete.partition_id,
                    },
                    delete.key.clone(),
                ),
                XLWrite::Update(_) => {
                    worker_rollback_tx(&self.tx_lock, &self.snapshot_mgr, tx_mgr)?;
                    return Err(mudu_error!(
                        ErrorCode::NotImplemented,
                        "commit_write_set does not support delta updates"
                    ));
                }
            }
        }
        self.worker_commit_tx_with_lock_owner_async(tx_mgr, lock_owner)
            .await
            .map(|_| ())
    }
}
