use super::*;
use crate::utils::{
    bound_key_as_ref, build_bound_key, build_key_tuple, ensure_supported_predicate,
    matches_predicate, project_selected_fields, rpc_bound_from_key_bound, staged_overlay_in_bounds,
};

/// Parameters for `WorkerXContract::read_range_remote_partition`.
struct RemoteRangeRead<'a> {
    worker_id: OID,
    table_id: OID,
    partition_id: OID,
    desc: &'a TableDesc,
    pred_key: &'a RangeData,
    pred_non_key: &'a Predicate,
    select: &'a VecSelTerm,
    start: &'a Bound<Vec<u8>>,
    end: &'a Bound<Vec<u8>>,
}

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
    pub(crate) async fn _read_key(
        &self,
        desc: Arc<TableDesc>,
        tx_mgr: Arc<dyn TxMgr>,
        table_id: OID,
        pred_key: &VecDatum,
        select: &VecSelTerm,
    ) -> RS<Option<Vec<Option<DataBin>>>> {
        let (key, target_partition) = {
            let _stage = ::catalog::stage_stats::StageGuard::new(
                ::catalog::stage_stats::Stage::ReadKeyRoute,
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
                    let _stage = ::catalog::stage_stats::StageGuard::new(
                        ::catalog::stage_stats::Stage::ReadKeyRoute,
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
                            let _stage = ::catalog::stage_stats::StageGuard::new(
                                ::catalog::stage_stats::Stage::ReadKeyRoute,
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
                            let _stage = ::catalog::stage_stats::StageGuard::new(
                                ::catalog::stage_stats::Stage::ReadKeyStorage,
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
                    let _stage = ::catalog::stage_stats::StageGuard::new(
                        ::catalog::stage_stats::Stage::ReadKeyStorage,
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
        let target_partitions = self
            .route_read_range_partitions(table_id, &desc, pred_key, pred_non_key)
            .await?;
        let mut projected = Vec::new();
        match target_partitions {
            Some(partitions) => {
                for partition_id in partitions {
                    match self.resolve_partition_worker(partition_id).await? {
                        Some(worker_id) if worker_id != self.worker_id => {
                            let rows = self
                                .read_range_remote_partition(
                                    tx_mgr.as_ref(),
                                    &RemoteRangeRead {
                                        worker_id,
                                        table_id,
                                        partition_id,
                                        desc: &desc,
                                        pred_key,
                                        pred_non_key,
                                        select,
                                        start: &start,
                                        end: &end,
                                    },
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
                            projected.extend(project_range_rows(
                                rows,
                                &desc,
                                pred_non_key,
                                select,
                            )?);
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
                projected.extend(project_range_rows(rows, &desc, pred_non_key, select)?);
            }
        }
        Ok(Arc::new(VecCursor {
            inner: SMutex::new(VecCursorInner {
                rows: projected,
                index: 0,
            }),
        }))
    }

    /// Route a range scan to its target partitions: a `KeyPrefixEq` predicate
    /// pinning every partition-key column resolves to the single owning
    /// partition; anything else fans out over the range router.
    async fn route_read_range_partitions(
        &self,
        table_id: OID,
        desc: &TableDesc,
        pred_key: &RangeData,
        pred_non_key: &Predicate,
    ) -> RS<Option<Vec<OID>>> {
        let target_partitions = match pred_non_key {
            Predicate::KeyPrefixEq(prefix) => {
                match self
                    .exact_partition_route_by_prefix(table_id, desc, prefix)
                    .await?
                {
                    Some(partition_id) => Some(vec![partition_id]),
                    None => {
                        self.partition_router
                            .route_range_partitions(
                                table_id,
                                desc,
                                pred_key.start(),
                                pred_key.end(),
                            )
                            .await?
                    }
                }
            }
            _ => {
                self.partition_router
                    .route_range_partitions(table_id, desc, pred_key.start(), pred_key.end())
                    .await?
            }
        };
        Ok(target_partitions)
    }

    /// Read one remote-owned partition of a range scan, merging the
    /// transaction's staged overlay (read-your-writes) when it overlaps the
    /// scan bounds or a key-prefix predicate needs client-side filtering.
    async fn read_range_remote_partition(
        &self,
        tx_mgr: &dyn TxMgr,
        read: &RemoteRangeRead<'_>,
    ) -> RS<Vec<Vec<Option<DataBin>>>> {
        let key_prefix = match read.pred_non_key {
            Predicate::KeyPrefixEq(prefix) => Some(prefix.as_slice()),
            _ => None,
        };
        let relation_id = PhysicalRelationId {
            table_id: read.table_id,
            partition_id: read.partition_id,
        };
        let overlay = staged_overlay_in_bounds(tx_mgr, relation_id, read.start, read.end);
        let rows = if overlay.is_empty() && key_prefix.is_none() {
            self.remote_read_range(
                read.worker_id,
                read.table_id,
                read.partition_id,
                rpc_bound_from_key_bound(read.pred_key.start(), read.desc)?,
                rpc_bound_from_key_bound(read.pred_key.end(), read.desc)?,
                read.select.vec().to_vec(),
            )
            .await?
        } else {
            self.remote_read_range_with_overlay(&RemoteRangeOverlayRead {
                worker_id: read.worker_id,
                table_id: read.table_id,
                partition_id: read.partition_id,
                desc: read.desc,
                pred_key: read.pred_key,
                select: read.select,
                overlay: &overlay,
                key_prefix,
            })
            .await?
        };
        Ok(rows)
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

/// Filter locally read range rows against the non-key predicate and project
/// the selected columns, preserving the scan order.
fn project_range_rows(
    rows: Vec<(Vec<u8>, Vec<u8>)>,
    desc: &TableDesc,
    pred_non_key: &Predicate,
    select: &VecSelTerm,
) -> RS<Vec<TupleRow>> {
    let mut projected = Vec::new();
    for (key, value) in rows {
        if !matches_predicate(desc, &key, &value, pred_non_key)? {
            continue;
        }
        projected.push(TupleRow::new_nullable(project_selected_fields(
            desc, &key, &value, select,
        )?));
    }
    Ok(projected)
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
