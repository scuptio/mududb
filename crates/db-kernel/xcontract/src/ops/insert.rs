use super::locks::acquire_statement_lock;
use super::*;
use crate::contract::WorkerXContract;
use crate::lifecycle_cluster::ensure_partition_write_open;
use crate::utils::{build_key_tuple, build_value_tuple};
use crate::worker_storage::WorkerStorage;
use crate::x_lock_mgr::XLockMgr;

/// Insert into a locally owned (or unpartitioned) partition under the
/// statement-level write lock.
#[allow(clippy::too_many_arguments)]
async fn insert_local(
    tx_lock: &XLockMgr,
    storage: &Arc<WorkerStorage>,
    worker_id: OID,
    tx_mgr: &Arc<dyn TxMgr>,
    table_id: OID,
    target_partition: Option<OID>,
    key: Vec<u8>,
    value: Vec<u8>,
) -> RS<()> {
    debug!(
        worker_id,
        table_id,
        target_partition = ?target_partition,
        "insert checking existing key locally"
    );
    acquire_statement_lock(
        tx_lock,
        tx_mgr.as_ref(),
        PhysicalRelationId {
            table_id,
            partition_id: storage.physical_partition_id(target_partition),
        },
        key.clone(),
    )
    .await?;
    let contain_key = {
        let _stage =
            ::catalog::stage_stats::StageGuard::new(::catalog::stage_stats::Stage::WriteStmtRead);
        storage
            .get_on_partition(table_id, target_partition, &key, tx_mgr.as_ref())
            .await?
    };
    if contain_key.is_some() {
        Err(mudu_error!(ErrorCode::ObjectAlreadyExists, "existing key"))
    } else {
        debug!(
            worker_id,
            table_id,
            target_partition = ?target_partition,
            "insert writing key locally"
        );
        let _stage = ::catalog::stage_stats::StageGuard::new(
            ::catalog::stage_stats::Stage::WriteStmtStaging,
        );
        storage
            .put_on_partition(table_id, target_partition, key, value, tx_mgr.as_ref())
            .await
    }
}

impl WorkerXContract {
    /// Insert with explicit partition pinning: skips key-based routing and
    /// targets `target_partition` directly (`None`/`Some(0)` is the
    /// unpartitioned default). Used by the queue executors, whose row keys
    /// (`__seq`) do not carry the queue's partition-route column.
    pub(crate) async fn insert_pinned(
        &self,
        desc: Arc<TableDesc>,
        tx_mgr: Arc<dyn TxMgr>,
        table_id: OID,
        target_partition: Option<OID>,
        keys: &VecDatum,
        values: &VecDatum,
    ) -> RS<()> {
        let key = build_key_tuple(keys, &desc)?;
        let value = build_value_tuple(values, &desc)?;
        if let Some(partition_id) = target_partition {
            ensure_partition_write_open(&self.cluster_router, &self.meta_mgr, partition_id).await?;
            match self.resolve_partition_worker(partition_id).await? {
                Some(worker_id) if worker_id != self.worker_id => {
                    return self
                        .insert_remote_owned(&tx_mgr, table_id, partition_id, worker_id, key, value)
                        .await;
                }
                _ => {}
            }
        }
        insert_local(
            &self.tx_lock,
            &self.storage,
            self.worker_id,
            &tx_mgr,
            table_id,
            target_partition,
            key,
            value,
        )
        .await
    }

    pub(crate) async fn _insert(
        &self,
        desc: Arc<TableDesc>,
        tx_mgr: Arc<dyn TxMgr>,
        table_id: OID,
        keys: &VecDatum,
        values: &VecDatum,
        _opt_insert: &OptInsert,
    ) -> RS<()> {
        debug!(
            worker_id = self.worker_id,
            table_id,
            key_cols = keys.data().len(),
            value_cols = values.data().len(),
            "insert begin"
        );
        let target_partition = self
            .partition_router
            .route_exact_partition(table_id, desc.as_ref(), keys)
            .await?;
        debug!(
            worker_id = self.worker_id,
            table_id,
            target_partition = ?target_partition,
            "insert routed partition"
        );
        self.insert_pinned(desc, tx_mgr, table_id, target_partition, keys, values)
            .await
    }

    /// Insert into a partition owned by a remote worker: the staged overlay
    /// decides conflicts for an already-staged key, otherwise the statement
    /// lock on the owner doubles as the existence check (same round trip).
    async fn insert_remote_owned(
        &self,
        tx_mgr: &Arc<dyn TxMgr>,
        table_id: OID,
        partition_id: OID,
        worker_id: OID,
        key: Vec<u8>,
        value: Vec<u8>,
    ) -> RS<()> {
        debug!(
            worker_id = self.worker_id,
            table_id,
            partition_id,
            target_worker_id = worker_id,
            "insert staging write for remote worker"
        );
        let relation_id = PhysicalRelationId {
            table_id,
            partition_id,
        };
        match tx_mgr.get_relation(relation_id, &key) {
            // The transaction already staged this key: an existing
            // staged value conflicts, a staged delete is replaced by
            // the new insert below. The statement lock was taken
            // when that earlier write was staged.
            Some(staged) => {
                if staged.is_some() {
                    return Err(mudu_error!(ErrorCode::ObjectAlreadyExists, "existing key"));
                }
            }
            // Take the statement-level write lock on the owner
            // (and read the committed value in the same round
            // trip): an existing key conflicts, otherwise the
            // insert is staged.
            None => {
                let existing = {
                    let _stage = ::catalog::stage_stats::StageGuard::new(
                        ::catalog::stage_stats::Stage::WriteStmtRead,
                    );
                    self.lock_remote_key_for_update(
                        tx_mgr.as_ref(),
                        worker_id,
                        table_id,
                        partition_id,
                        key.clone(),
                        vec![],
                    )
                    .await?
                };
                if existing.is_some() {
                    return Err(mudu_error!(ErrorCode::ObjectAlreadyExists, "existing key"));
                }
            }
        }
        {
            let _stage = ::catalog::stage_stats::StageGuard::new(
                ::catalog::stage_stats::Stage::WriteStmtStaging,
            );
            tx_mgr.put_relation(relation_id, key, value);
        }
        Ok(())
    }
}
