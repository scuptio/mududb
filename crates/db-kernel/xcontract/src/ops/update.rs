use super::*;
use crate::utils::{
    apply_value_update_with_deltas, build_key_tuple, build_value_tuple, ensure_supported_predicate,
};

/// The update payload routed through `_update`: absolute `values`,
/// expression-assignment `deltas`, and explicit NULL assignments.
struct UpdatePayload<'a> {
    values: &'a VecDatum,
    deltas: &'a [DeltaAssign],
    null_assignments: &'a [AttrIndex],
}

impl WorkerXContract {
    // The update payload is split into absolute `values` and `deltas`
    // (expression assignments) to keep `VecDatum` unchanged across the
    // storage stack; bundling them would not reduce the real complexity.
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn _update(
        &self,
        desc: Arc<TableDesc>,
        tx_mgr: Arc<dyn TxMgr>,
        table_id: OID,
        pred_key: &VecDatum,
        pred_non_key: &Predicate,
        values: &VecDatum,
        deltas: &[DeltaAssign],
        null_assignments: &[AttrIndex],
    ) -> RS<usize> {
        ensure_supported_predicate(pred_non_key)?;
        let key = build_key_tuple(pred_key, &desc)?;
        let target_partition = self
            .partition_router
            .route_exact_partition(table_id, desc.as_ref(), pred_key)
            .await?;
        // Deferred (apply-time, lock-free) deltas: the caller opted into
        // commutative apply-time evaluation for every assignment in this
        // call, so no statement lock and no statement-time read are needed.
        // They cannot be mixed with absolute assignments (the staged
        // absolute value would silently discard a concurrent commutative
        // update) nor applied to remote-owned partitions (the deferred apply
        // runs locally).
        let payload = UpdatePayload {
            values,
            deltas,
            null_assignments,
        };
        let has_deferred = payload.deltas.iter().any(|assign| assign.op.is_deferred());
        if has_deferred {
            return self
                .update_deferred_deltas(&tx_mgr, table_id, target_partition, key, &payload)
                .await;
        }
        if let Some(partition_id) = target_partition {
            match self.resolve_partition_worker(partition_id).await? {
                Some(worker_id) if worker_id != self.worker_id => {
                    return self
                        .update_remote_owned(
                            &desc,
                            &tx_mgr,
                            table_id,
                            partition_id,
                            worker_id,
                            key,
                            &payload,
                        )
                        .await;
                }
                _ => {}
            }
        }
        self.update_local(&desc, &tx_mgr, table_id, target_partition, key, &payload)
            .await
    }

    /// Stage deferred (apply-time, lock-free) delta assignments. Every
    /// assignment must be deferred — they cannot be mixed with absolute
    /// assignments (the staged absolute value would silently discard a
    /// concurrent commutative update) — and the partition must be locally
    /// owned (the deferred apply runs locally).
    async fn update_deferred_deltas(
        &self,
        tx_mgr: &Arc<dyn TxMgr>,
        table_id: OID,
        target_partition: Option<OID>,
        key: Vec<u8>,
        payload: &UpdatePayload<'_>,
    ) -> RS<usize> {
        if !payload.values.data().is_empty()
            || !payload.null_assignments.is_empty()
            || payload.deltas.iter().any(|assign| !assign.op.is_deferred())
        {
            return Err(mudu_error!(
                ErrorCode::InvalidArgument,
                "deferred delta assignments cannot be mixed with absolute assignments or statement-time deltas"
            ));
        }
        let mut deferred_remote_owner = None;
        if let Some(partition_id) = target_partition {
            if let Some(worker_id) = self.resolve_partition_worker(partition_id).await? {
                if worker_id != self.worker_id {
                    deferred_remote_owner = Some((partition_id, worker_id));
                }
            }
        }
        if let Some((partition_id, worker_id)) = deferred_remote_owner {
            return Err(mudu_error!(
                ErrorCode::InvalidArgument,
                format!(
                    "deferred delta assignments are only supported on the local worker (partition {} is owned by worker {})",
                    partition_id, worker_id
                )
            ));
        }
        let relation_id = PhysicalRelationId {
            table_id,
            partition_id: self.storage.physical_partition_id(target_partition),
        };
        // Unlocked existence check (READ COMMITTED): a missing row means
        // zero affected rows, matching the locked path.
        let current = {
            let _stage = ::catalog::stage_stats::StageGuard::new(
                ::catalog::stage_stats::Stage::WriteStmtRead,
            );
            self.storage
                .get_on_partition(table_id, target_partition, &key, tx_mgr.as_ref())
                .await?
        };
        if current.is_none() {
            return Ok(0);
        }
        {
            let _stage = ::catalog::stage_stats::StageGuard::new(
                ::catalog::stage_stats::Stage::WriteStmtStaging,
            );
            tx_mgr.put_relation_deferred_deltas(relation_id, key, payload.deltas.to_vec())?;
        }
        Ok(1)
    }

    /// Update a row of a remote-owned partition: read-modify-write under the
    /// statement-level write lock taken on the owner (lock + read in one
    /// round trip), then stage the updated value.
    // The routed target (table, partition, owner worker) travels as separate
    // arguments like the `_update` caller; bundling them would not reduce
    // the real complexity.
    #[allow(clippy::too_many_arguments)]
    async fn update_remote_owned(
        &self,
        desc: &TableDesc,
        tx_mgr: &Arc<dyn TxMgr>,
        table_id: OID,
        partition_id: OID,
        worker_id: OID,
        key: Vec<u8>,
        payload: &UpdatePayload<'_>,
    ) -> RS<usize> {
        let relation_id = PhysicalRelationId {
            table_id,
            partition_id,
        };
        let current = match tx_mgr.get_relation(relation_id, &key) {
            Some(staged) => staged,
            None => {
                // Read-modify-write under the statement-level
                // write lock: lock on the owner and fetch the
                // committed value in the same round trip, then
                // stage the updated value.
                let value_attrs = desc.value_indices().clone();
                let row = {
                    let _stage = ::catalog::stage_stats::StageGuard::new(
                        ::catalog::stage_stats::Stage::WriteStmtRead,
                    );
                    self.lock_remote_key_for_update(
                        tx_mgr.as_ref(),
                        worker_id,
                        table_id,
                        partition_id,
                        key.clone(),
                        value_attrs.clone(),
                    )
                    .await?
                };
                match row {
                    Some(fields) => {
                        let data = value_attrs
                            .into_iter()
                            .zip(fields)
                            .filter_map(|(attr, field)| field.map(|field| (attr, field)))
                            .collect();
                        Some(build_value_tuple(&VecDatum::new(data), desc)?)
                    }
                    None => None,
                }
            }
        };
        let Some(current) = current else {
            return Ok(0);
        };
        let updated = apply_value_update_with_deltas(
            &current,
            payload.values,
            payload.deltas,
            payload.null_assignments,
            desc,
        )?;
        {
            let _stage = ::catalog::stage_stats::StageGuard::new(
                ::catalog::stage_stats::Stage::WriteStmtStaging,
            );
            tx_mgr.put_relation(relation_id, key, updated);
        }
        Ok(1)
    }

    /// Update a row of a locally owned (or unpartitioned) partition under
    /// the statement-level write lock: read the current value, apply the
    /// update payload, and stage the result.
    async fn update_local(
        &self,
        desc: &TableDesc,
        tx_mgr: &Arc<dyn TxMgr>,
        table_id: OID,
        target_partition: Option<OID>,
        key: Vec<u8>,
        payload: &UpdatePayload<'_>,
    ) -> RS<usize> {
        self.acquire_statement_lock(
            tx_mgr.as_ref(),
            PhysicalRelationId {
                table_id,
                partition_id: self.storage.physical_partition_id(target_partition),
            },
            key.clone(),
        )
        .await?;
        let current = {
            let _stage = ::catalog::stage_stats::StageGuard::new(
                ::catalog::stage_stats::Stage::WriteStmtRead,
            );
            self.storage
                .get_on_partition(table_id, target_partition, &key, tx_mgr.as_ref())
                .await?
        };
        let Some(current) = current else {
            return Ok(0);
        };
        let updated = apply_value_update_with_deltas(
            &current,
            payload.values,
            payload.deltas,
            payload.null_assignments,
            desc,
        )?;
        {
            let _stage = ::catalog::stage_stats::StageGuard::new(
                ::catalog::stage_stats::Stage::WriteStmtStaging,
            );
            self.storage
                .put_on_partition(table_id, target_partition, key, updated, tx_mgr.as_ref())
                .await
                .map(|()| 1)
        }
    }
}
