use super::*;
use crate::utils::{build_key_tuple, ensure_supported_predicate};

impl WorkerXContract {
    pub(crate) async fn _delete(
        &self,
        desc: Arc<TableDesc>,
        tx_mgr: Arc<dyn TxMgr>,
        table_id: OID,
        pred_key: &VecDatum,
        pred_non_key: &Predicate,
        _opt_delete: &OptDelete,
    ) -> RS<usize> {
        ensure_supported_predicate(pred_non_key)?;
        let key = build_key_tuple(pred_key, &desc)?;
        let target_partition = self
            .partition_router
            .route_exact_partition(table_id, desc.as_ref(), pred_key)
            .await?;
        if let Some(partition_id) = target_partition {
            match self.resolve_partition_worker(partition_id).await? {
                Some(worker_id) if worker_id != self.worker_id => {
                    let relation_id = PhysicalRelationId {
                        table_id,
                        partition_id,
                    };
                    let exists = match tx_mgr.get_relation(relation_id, &key) {
                        Some(staged) => staged.is_some(),
                        // Lock on the owner first, then decide from the
                        // committed value read in the same round trip.
                        None => self
                            .lock_remote_key_for_update(
                                tx_mgr.as_ref(),
                                worker_id,
                                table_id,
                                partition_id,
                                key.clone(),
                                vec![],
                            )
                            .await?
                            .is_some(),
                    };
                    if !exists {
                        return Ok(0);
                    }
                    tx_mgr.delete_relation(relation_id, key);
                    return Ok(1);
                }
                _ => {}
            }
        }
        self.acquire_statement_lock(
            tx_mgr.as_ref(),
            PhysicalRelationId {
                table_id,
                partition_id: self.storage.physical_partition_id(target_partition),
            },
            key.clone(),
        )
        .await?;
        let deleted = self
            .storage
            .remove_on_partition(table_id, target_partition, &key, tx_mgr.as_ref())
            .await?;
        Ok(usize::from(deleted.is_some()))
    }
}
