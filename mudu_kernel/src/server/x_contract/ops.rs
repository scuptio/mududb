use super::utils::*;
use super::*;

impl WorkerXContract {
    pub(crate) fn _begin_tx(&self) -> RS<Arc<dyn TxMgr>> {
        Ok(Arc::new(WorkerTxManager::new(
            self.snapshot_mgr.begin_tx()?,
        )))
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
        let key = build_key_tuple(keys, &desc)?;
        let value = build_value_tuple(values, &desc)?;
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
        if let Some(partition_id) = target_partition {
            match self.resolve_partition_worker(partition_id).await? {
                Some(worker_id) if self.worker_id != 0 && worker_id != self.worker_id => {
                    debug!(
                        worker_id = self.worker_id,
                        table_id,
                        partition_id,
                        target_worker_id = worker_id,
                        "insert forwarding to remote worker"
                    );
                    return self
                        .remote_insert(worker_id, table_id, partition_id, key, value)
                        .await;
                }
                _ => {}
            }
        }
        debug!(
            worker_id = self.worker_id,
            table_id,
            target_partition = ?target_partition,
            "insert checking existing key locally"
        );
        let contain_key = self
            .storage
            .get_on_partition(table_id, target_partition, &key, tx_mgr.as_ref())
            .await?;
        if contain_key.is_some() {
            Err(mudu_error!(ErrorCode::EntityAlreadyExists, "existing key"))
        } else {
            debug!(
                worker_id = self.worker_id,
                table_id,
                target_partition = ?target_partition,
                "insert writing key locally"
            );
            self.storage
                .put_on_partition(table_id, target_partition, key, value, tx_mgr.as_ref())
                .await
        }
    }

    pub(crate) async fn _read_key(
        &self,
        desc: Arc<TableDesc>,
        tx_mgr: Arc<dyn TxMgr>,
        table_id: OID,
        pred_key: &VecDatum,
        select: &VecSelTerm,
    ) -> RS<Option<Vec<Option<DatBin>>>> {
        let key = build_key_tuple(pred_key, &desc)?;
        let target_partition = self
            .partition_router
            .route_exact_partition(table_id, desc.as_ref(), pred_key)
            .await?;
        let opt_value = match target_partition {
            Some(partition_id) => match self.resolve_partition_worker(partition_id).await? {
                Some(worker_id) if self.worker_id != 0 && worker_id != self.worker_id => {
                    self.remote_read_key(
                        worker_id,
                        table_id,
                        partition_id,
                        key.clone(),
                        select.vec().to_vec(),
                    )
                    .await?
                }
                _ => {
                    let result = self
                        .storage
                        .get_on_partition(table_id, Some(partition_id), &key, tx_mgr.as_ref())
                        .await?;
                    result
                        .map(|value| project_selected_fields(&desc, &key, &value, select))
                        .transpose()?
                }
            },
            None => {
                let result = self
                    .storage
                    .get_on_partition(table_id, None, &key, tx_mgr.as_ref())
                    .await?;
                result
                    .map(|value| project_selected_fields(&desc, &key, &value, select))
                    .transpose()?
            }
        };
        match opt_value {
            Some(value) => Ok(Some(value)),
            None => Ok(None),
        }
    }

    pub(crate) async fn _read_range(
        &self,
        desc: Arc<TableDesc>,
        tx_mgr: Arc<dyn TxMgr>,
        table_id: OID,
        pred_key: &RangeData,
        pred_non_key: &Predicate,
        select: &VecSelTerm,
    ) -> RS<Arc<dyn RSCursor>> {
        ensure_supported_predicate(pred_non_key)?;
        let start = build_bound_key(pred_key.start(), &desc)?;
        let end = build_bound_key(pred_key.end(), &desc)?;
        let target_partitions = self
            .partition_router
            .route_range_partitions(table_id, desc.as_ref(), pred_key.start(), pred_key.end())
            .await?;
        let mut projected = Vec::new();
        match target_partitions {
            Some(partitions) => {
                for partition_id in partitions {
                    match self.resolve_partition_worker(partition_id).await? {
                        Some(worker_id) if self.worker_id != 0 && worker_id != self.worker_id => {
                            if matches!(pred_non_key, Predicate::KeyPrefixEq(_)) {
                                return Err(mudu_error!(
                                    ErrorCode::NotImplemented,
                                    "key-prefix range filtering is not implemented for remote partitions"
                                ));
                            }
                            let rows = self
                                .remote_read_range(
                                    worker_id,
                                    table_id,
                                    partition_id,
                                    rpc_bound_from_key_bound(pred_key.start(), &desc)?,
                                    rpc_bound_from_key_bound(pred_key.end(), &desc)?,
                                    select.vec().to_vec(),
                                )
                                .await?;
                            for row in rows {
                                projected.push(TupleRow::new_nullable(row));
                            }
                        }
                        _ => {
                            let rows = self
                                .storage
                                .range_on_partition(
                                    table_id,
                                    Some(partition_id),
                                    (bound_key_as_ref(&start), bound_key_as_ref(&end)),
                                    tx_mgr.as_ref(),
                                )
                                .await?;
                            for (key, value) in rows {
                                if !matches_predicate(&desc, &key, &value, pred_non_key)? {
                                    continue;
                                }
                                projected.push(TupleRow::new_nullable(project_selected_fields(
                                    &desc, &key, &value, select,
                                )?));
                            }
                        }
                    }
                }
            }
            None => {
                let rows = self
                    .storage
                    .range(
                        table_id,
                        (bound_key_as_ref(&start), bound_key_as_ref(&end)),
                        tx_mgr.as_ref(),
                    )
                    .await?;
                for (key, value) in rows {
                    if !matches_predicate(&desc, &key, &value, pred_non_key)? {
                        continue;
                    }
                    projected.push(TupleRow::new_nullable(project_selected_fields(
                        &desc, &key, &value, select,
                    )?));
                }
            }
        }
        Ok(Arc::new(VecCursor {
            inner: SMutex::new(VecCursorInner {
                rows: projected,
                index: 0,
            }),
        }))
    }

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
                Some(worker_id) if self.worker_id != 0 && worker_id != self.worker_id => {
                    return self
                        .remote_delete(worker_id, table_id, partition_id, key)
                        .await;
                }
                _ => {}
            }
        }
        let deleted = self
            .storage
            .remove_on_partition(table_id, target_partition, &key, tx_mgr.as_ref())
            .await?;
        Ok(usize::from(deleted.is_some()))
    }

    pub(crate) async fn _update(
        &self,
        desc: Arc<TableDesc>,
        tx_mgr: Arc<dyn TxMgr>,
        table_id: OID,
        pred_key: &VecDatum,
        pred_non_key: &Predicate,
        values: &VecDatum,
    ) -> RS<usize> {
        ensure_supported_predicate(pred_non_key)?;
        let key = build_key_tuple(pred_key, &desc)?;
        let target_partition = self
            .partition_router
            .route_exact_partition(table_id, desc.as_ref(), pred_key)
            .await?;
        if let Some(partition_id) = target_partition {
            match self.resolve_partition_worker(partition_id).await? {
                Some(worker_id) if self.worker_id != 0 && worker_id != self.worker_id => {
                    return self
                        .remote_update(
                            worker_id,
                            table_id,
                            partition_id,
                            key,
                            values.data().clone(),
                        )
                        .await;
                }
                _ => {}
            }
        }
        let current = self
            .storage
            .get_on_partition(table_id, target_partition, &key, tx_mgr.as_ref())
            .await?;
        let Some(current) = current else {
            return Ok(0);
        };
        let updated = apply_value_update(&current, values, &desc)?;
        self.storage
            .put_on_partition(table_id, target_partition, key, updated, tx_mgr.as_ref())
            .await
            .map(|()| 1)
    }
}
