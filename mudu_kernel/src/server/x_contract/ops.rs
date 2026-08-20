use super::utils::*;
use super::*;

/// Parameters for `WorkerXContract::remote_read_range_with_overlay`.
struct RemoteRangeOverlayRead<'a> {
    worker_id: OID,
    table_id: OID,
    partition_id: OID,
    desc: &'a TableDesc,
    pred_key: &'a RangeData,
    select: &'a VecSelTerm,
    overlay: &'a [(Vec<u8>, Option<Vec<u8>>)],
    /// Optional `Predicate::KeyPrefixEq` prefix, applied to the decoded key
    /// datums of each merged row.
    key_prefix: Option<&'a [(AttrIndex, DataBin)]>,
}

impl WorkerXContract {
    /// Take a statement-level pessimistic write lock on `key` of
    /// `relation_id` on the local worker (waiting, bounded by
    /// `STATEMENT_LOCK_TIMEOUT`) and record it on the transaction for
    /// commit/rollback release. Re-entrant for keys the transaction already
    /// holds.
    async fn acquire_statement_lock(
        &self,
        tx_mgr: &dyn TxMgr,
        relation_id: PhysicalRelationId,
        key: Vec<u8>,
    ) -> RS<()> {
        let xid = tx_mgr.xid();
        let acquired = {
            let _stage = crate::server::stage_stats::StageGuard::new(
                crate::server::stage_stats::Stage::StmtLock,
            );
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
    async fn lock_remote_key_for_update(
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
                Some(worker_id) if worker_id != self.worker_id => {
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
                                return Err(mudu_error!(
                                    ErrorCode::EntityAlreadyExists,
                                    "existing key"
                                ));
                            }
                        }
                        // Take the statement-level write lock on the owner
                        // (and read the committed value in the same round
                        // trip): an existing key conflicts, otherwise the
                        // insert is staged.
                        None => {
                            let existing = {
                                let _stage = crate::server::stage_stats::StageGuard::new(
                                    crate::server::stage_stats::Stage::WriteStmtRead,
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
                                return Err(mudu_error!(
                                    ErrorCode::EntityAlreadyExists,
                                    "existing key"
                                ));
                            }
                        }
                    }
                    {
                        let _stage = crate::server::stage_stats::StageGuard::new(
                            crate::server::stage_stats::Stage::WriteStmtStaging,
                        );
                        tx_mgr.put_relation(relation_id, key, value);
                    }
                    return Ok(());
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
        self.acquire_statement_lock(
            tx_mgr.as_ref(),
            PhysicalRelationId {
                table_id,
                partition_id: self.storage.physical_partition_id(target_partition),
            },
            key.clone(),
        )
        .await?;
        let contain_key = {
            let _stage = crate::server::stage_stats::StageGuard::new(
                crate::server::stage_stats::Stage::WriteStmtRead,
            );
            self.storage
                .get_on_partition(table_id, target_partition, &key, tx_mgr.as_ref())
                .await?
        };
        if contain_key.is_some() {
            Err(mudu_error!(ErrorCode::EntityAlreadyExists, "existing key"))
        } else {
            debug!(
                worker_id = self.worker_id,
                table_id,
                target_partition = ?target_partition,
                "insert writing key locally"
            );
            let _stage = crate::server::stage_stats::StageGuard::new(
                crate::server::stage_stats::Stage::WriteStmtStaging,
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
    ) -> RS<Option<Vec<Option<DataBin>>>> {
        let (key, target_partition) = {
            let _stage = crate::server::stage_stats::StageGuard::new(
                crate::server::stage_stats::Stage::ReadKeyRoute,
            );
            let key = build_key_tuple(pred_key, &desc)?;
            let target_partition = self
                .partition_router
                .route_exact_partition(table_id, desc.as_ref(), pred_key)
                .await?;
            (key, target_partition)
        };
        let opt_value = match target_partition {
            Some(partition_id) => {
                let resolved_worker = {
                    let _stage = crate::server::stage_stats::StageGuard::new(
                        crate::server::stage_stats::Stage::ReadKeyRoute,
                    );
                    self.resolve_partition_worker(partition_id).await?
                };
                match resolved_worker {
                    Some(worker_id) if worker_id != self.worker_id => {
                        let relation_id = PhysicalRelationId {
                            table_id,
                            partition_id,
                        };
                        let staged_overlay = {
                            let _stage = crate::server::stage_stats::StageGuard::new(
                                crate::server::stage_stats::Stage::ReadKeyRoute,
                            );
                            tx_mgr.get_relation(relation_id, &key)
                        };
                        match staged_overlay {
                            // Read-your-writes: a staged delete reads as missing.
                            Some(staged) => staged
                                .map(|value| project_selected_fields(&desc, &key, &value, select))
                                .transpose()?,
                            None => {
                                self.remote_read_key(
                                    worker_id,
                                    table_id,
                                    partition_id,
                                    key.clone(),
                                    select.vec().to_vec(),
                                )
                                .await?
                            }
                        }
                    }
                    _ => {
                        let result = {
                            let _stage = crate::server::stage_stats::StageGuard::new(
                                crate::server::stage_stats::Stage::ReadKeyStorage,
                            );
                            self.storage
                                .get_on_partition(
                                    table_id,
                                    Some(partition_id),
                                    &key,
                                    tx_mgr.as_ref(),
                                )
                                .await?
                        };
                        result
                            .map(|value| project_selected_fields(&desc, &key, &value, select))
                            .transpose()?
                    }
                }
            }
            None => {
                let result = {
                    let _stage = crate::server::stage_stats::StageGuard::new(
                        crate::server::stage_stats::Stage::ReadKeyStorage,
                    );
                    self.storage
                        .get_on_partition(table_id, None, &key, tx_mgr.as_ref())
                        .await?
                };
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

    /// If `prefix` (from a `Predicate::KeyPrefixEq`) pins every partition-key
    /// column of `table_id` with an equality value, return the single owning
    /// partition. This keeps prefix-equality scans on one partition instead
    /// of fanning the unbounded range out to every partition (one
    /// cross-worker RPC per partition).
    async fn exact_partition_route_by_prefix(
        &self,
        table_id: OID,
        desc: &TableDesc,
        prefix: &[(AttrIndex, DataBin)],
    ) -> RS<Option<OID>> {
        let Some(binding) = self.meta_mgr.get_table_partition_binding(table_id).await? else {
            return Ok(None);
        };
        if !PartitionRouter::prefix_covers_partition_key(&binding.ref_attr_indices, prefix) {
            return Ok(None);
        }
        self.partition_router
            .route_exact_partition(table_id, desc, &VecDatum::new(prefix.to_vec()))
            .await
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
        let target_partitions = match pred_non_key {
            Predicate::KeyPrefixEq(prefix) => {
                match self
                    .exact_partition_route_by_prefix(table_id, desc.as_ref(), prefix)
                    .await?
                {
                    Some(partition_id) => Some(vec![partition_id]),
                    None => {
                        self.partition_router
                            .route_range_partitions(
                                table_id,
                                desc.as_ref(),
                                pred_key.start(),
                                pred_key.end(),
                            )
                            .await?
                    }
                }
            }
            _ => {
                self.partition_router
                    .route_range_partitions(
                        table_id,
                        desc.as_ref(),
                        pred_key.start(),
                        pred_key.end(),
                    )
                    .await?
            }
        };
        let mut projected = Vec::new();
        match target_partitions {
            Some(partitions) => {
                for partition_id in partitions {
                    match self.resolve_partition_worker(partition_id).await? {
                        Some(worker_id) if worker_id != self.worker_id => {
                            let key_prefix = match pred_non_key {
                                Predicate::KeyPrefixEq(prefix) => Some(prefix.as_slice()),
                                _ => None,
                            };
                            let relation_id = PhysicalRelationId {
                                table_id,
                                partition_id,
                            };
                            let overlay = staged_overlay_in_bounds(
                                tx_mgr.as_ref(),
                                relation_id,
                                &start,
                                &end,
                            );
                            let rows = if overlay.is_empty() && key_prefix.is_none() {
                                self.remote_read_range(
                                    worker_id,
                                    table_id,
                                    partition_id,
                                    rpc_bound_from_key_bound(pred_key.start(), &desc)?,
                                    rpc_bound_from_key_bound(pred_key.end(), &desc)?,
                                    select.vec().to_vec(),
                                )
                                .await?
                            } else {
                                self.remote_read_range_with_overlay(&RemoteRangeOverlayRead {
                                    worker_id,
                                    table_id,
                                    partition_id,
                                    desc: &desc,
                                    pred_key,
                                    select,
                                    overlay: &overlay,
                                    key_prefix,
                                })
                                .await?
                            };
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
        let has_deferred = deltas.iter().any(|assign| assign.op.is_deferred());
        if has_deferred {
            if !values.data().is_empty() || deltas.iter().any(|assign| !assign.op.is_deferred()) {
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
                let _stage = crate::server::stage_stats::StageGuard::new(
                    crate::server::stage_stats::Stage::WriteStmtRead,
                );
                self.storage
                    .get_on_partition(table_id, target_partition, &key, tx_mgr.as_ref())
                    .await?
            };
            if current.is_none() {
                return Ok(0);
            }
            {
                let _stage = crate::server::stage_stats::StageGuard::new(
                    crate::server::stage_stats::Stage::WriteStmtStaging,
                );
                tx_mgr.put_relation_deferred_deltas(relation_id, key, deltas.to_vec())?;
            }
            return Ok(1);
        }
        if let Some(partition_id) = target_partition {
            match self.resolve_partition_worker(partition_id).await? {
                Some(worker_id) if worker_id != self.worker_id => {
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
                                let _stage = crate::server::stage_stats::StageGuard::new(
                                    crate::server::stage_stats::Stage::WriteStmtRead,
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
                                        .filter_map(|(attr, field)| {
                                            field.map(|field| (attr, field))
                                        })
                                        .collect();
                                    Some(build_value_tuple(&VecDatum::new(data), &desc)?)
                                }
                                None => None,
                            }
                        }
                    };
                    let Some(current) = current else {
                        return Ok(0);
                    };
                    let updated = apply_value_update_with_deltas(&current, values, deltas, &desc)?;
                    {
                        let _stage = crate::server::stage_stats::StageGuard::new(
                            crate::server::stage_stats::Stage::WriteStmtStaging,
                        );
                        tx_mgr.put_relation(relation_id, key, updated);
                    }
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
        let current = {
            let _stage = crate::server::stage_stats::StageGuard::new(
                crate::server::stage_stats::Stage::WriteStmtRead,
            );
            self.storage
                .get_on_partition(table_id, target_partition, &key, tx_mgr.as_ref())
                .await?
        };
        let Some(current) = current else {
            return Ok(0);
        };
        let updated = apply_value_update_with_deltas(&current, values, deltas, &desc)?;
        {
            let _stage = crate::server::stage_stats::StageGuard::new(
                crate::server::stage_stats::Stage::WriteStmtStaging,
            );
            self.storage
                .put_on_partition(table_id, target_partition, key, updated, tx_mgr.as_ref())
                .await
                .map(|()| 1)
        }
    }

    /// Read a remote partition range and merge the transaction's staged
    /// overlay so the scan observes its own writes (read-your-writes).
    ///
    /// The remote scan is requested with the key attributes prepended to
    /// `select` so each returned row can be matched against staged keys; the
    /// extra key columns are stripped before returning. Merged rows are
    /// emitted ordered by their decoded key datums, which approximates the
    /// raw key ordering of a purely local scan.
    async fn remote_read_range_with_overlay(
        &self,
        read: &RemoteRangeOverlayRead<'_>,
    ) -> RS<Vec<Vec<Option<DataBin>>>> {
        let desc = read.desc;
        let select = read.select;
        let key_attrs = desc.key_indices().clone();
        let mut extra_attrs: Vec<AttrIndex> = Vec::new();
        for attr in select.vec() {
            if !key_attrs.contains(attr) && !extra_attrs.contains(attr) {
                extra_attrs.push(*attr);
            }
        }
        let mut augmented = key_attrs.clone();
        augmented.extend(extra_attrs.iter().copied());
        let rows = self
            .remote_read_range(
                read.worker_id,
                read.table_id,
                read.partition_id,
                rpc_bound_from_key_bound(read.pred_key.start(), desc)?,
                rpc_bound_from_key_bound(read.pred_key.end(), desc)?,
                augmented,
            )
            .await?;
        // Position of each originally selected attribute inside the augmented
        // projection (key attributes first, then the non-key extras).
        let mut select_positions = Vec::with_capacity(select.vec().len());
        for attr in select.vec() {
            let position = match key_attrs.iter().position(|candidate| candidate == attr) {
                Some(position) => position,
                None => {
                    key_attrs.len()
                        + extra_attrs
                            .iter()
                            .position(|candidate| candidate == attr)
                            .ok_or_else(|| {
                                mudu_error!(
                                    ErrorCode::Internal,
                                    "selected attribute missing from augmented projection"
                                )
                            })?
                }
            };
            select_positions.push(position);
        }
        let key_signature = |key: &[u8]| -> RS<Vec<Option<DataBin>>> {
            let mut signature = Vec::with_capacity(key_attrs.len());
            for attr in &key_attrs {
                let index = desc.get_attr(*attr).datum_index();
                let field_desc = desc.key_desc().get_field_desc(index);
                signature.push(Some(field_desc.get(key)?.to_vec()));
            }
            Ok(signature)
        };
        let mut merged: BTreeMap<Vec<Option<DataBin>>, Vec<Option<DataBin>>> = BTreeMap::new();
        for row in rows {
            let signature = row[..key_attrs.len()].to_vec();
            let payload = select_positions
                .iter()
                .map(|position| row[*position].clone())
                .collect();
            merged.insert(signature, payload);
        }
        for (key, opt_value) in read.overlay {
            let signature = key_signature(key)?;
            match opt_value {
                Some(value) => {
                    let payload = project_selected_fields(desc, key, value, select)?;
                    merged.insert(signature, payload);
                }
                None => {
                    merged.remove(&signature);
                }
            }
        }
        if let Some(prefix) = read.key_prefix {
            merged.retain(|signature, _| key_prefix_matches(&key_attrs, signature, prefix));
        }
        Ok(merged.into_values().collect())
    }
}

/// Equality test for a `Predicate::KeyPrefixEq` prefix against a row's
/// decoded key datums (the same byte encoding `matches_predicate` compares
/// on raw keys).
fn key_prefix_matches(
    key_attrs: &[AttrIndex],
    signature: &[Option<DataBin>],
    prefix: &[(AttrIndex, DataBin)],
) -> bool {
    prefix.iter().all(|(attr, expected)| {
        key_attrs
            .iter()
            .position(|candidate| candidate == attr)
            .and_then(|position| signature.get(position))
            .is_some_and(|actual| actual.as_ref() == Some(expected))
    })
}
