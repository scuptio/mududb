//! Migration handlers of the partition RPC service: the source owner offers its
//! partition's relation files (snapshot manifest + paged fetches), the
//! target worker pulls and installs them (`PartitionAdopt`), and the source
//! drops its copy after the placement flip (`PartitionDrop`).

use super::*;
use crate::kv::tx::{worker_begin_tx_reading_current, worker_rollback_tx};
use crate::lifecycle_cluster::migration_inflight_count;
use crate::worker_storage::WorkerStorage;
use ::catalog::contract::cluster_router::ClusterRouter;
use ::catalog::contract::meta_mgr::MetaMgr;
use mudu::common::crc::crc64;
use mudu_sys::contract::file_options::FileOptions;
use std::path::{Path, PathBuf};

/// Byte cap of one snapshot fetch page.
const SNAPSHOT_FETCH_PAGE: u64 = 4 * 1024 * 1024;

/// The relation directory of a data dir (relation files are named
/// `<partition_id>.<relation>.dat`).
fn relation_dir(data_dir: &str) -> PathBuf {
    Path::new(data_dir).join("relation")
}

/// Validates a snapshot fetch path: must name a data file of
/// `partition_id` inside the relation directory (no traversal).
fn validate_snapshot_path(partition_id: OID, path: &str) -> RS<String> {
    let prefix = format!("relation/{partition_id}.");
    if !path.starts_with(&prefix) || !path.ends_with(".dat") || path.contains("..") {
        return Err(mudu_error!(
            ErrorCode::InvalidArgument,
            format!("invalid partition snapshot path {path}")
        ));
    }
    Ok(path.to_string())
}

/// Serve `PartitionSnapshotOffer`: flush this worker's dirty pages so
/// the partition's relation files are current, then return their
/// manifest `(path, len, crc64)`.
async fn execute_rpc_snapshot_offer(
    storage: &Arc<WorkerStorage>,
    worker_id: OID,
    partition_id: OID,
) -> RS<PartitionRpcResponse> {
    debug!(
        worker_id,
        partition_id, "execute partition rpc snapshot_offer"
    );
    storage.flush_dirty_pages_async().await?;
    let dir = relation_dir(storage.relation_path());
    let fs = mudu_sys::default_sys_io_context().provider_arc().fs_arc();
    let mut files = Vec::new();
    if fs.path_exists(&dir).await? {
        for path in fs.read_dir(&dir).await? {
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            if !name.starts_with(&format!("{partition_id}.")) || !name.ends_with(".dat") {
                continue;
            }
            let bytes = fs.read_all(&path).await?;
            files.push((
                format!("relation/{name}"),
                bytes.len() as u64,
                crc64(&bytes),
            ));
        }
    }
    Ok(PartitionRpcResponse::PartitionSnapshotOffer(files))
}

/// Serve `PartitionDrop` (post-move cleanup): evict the partition's
/// open relations, delete its data files, and release this worker's
/// ownership registration.
async fn execute_rpc_partition_drop(
    storage: &Arc<WorkerStorage>,
    worker_id: OID,
    partition_adoption: Option<Arc<dyn ::catalog::contract::partition_adoption::PartitionAdoption>>,
    partition_id: OID,
) -> RS<PartitionRpcResponse> {
    debug!(worker_id, partition_id, "execute partition rpc drop");
    storage
        .evict_partition_relations_async(partition_id)
        .await?;
    let dir = relation_dir(storage.relation_path());
    let fs = mudu_sys::default_sys_io_context().provider_arc().fs_arc();
    if fs.path_exists(&dir).await? {
        for path in fs.read_dir(&dir).await? {
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            if name.starts_with(&format!("{partition_id}.")) && name.ends_with(".dat") {
                fs.remove_file_if_exists(&path).await?;
            }
        }
    }
    if let Some(adoption) = partition_adoption {
        adoption.release(partition_id)?;
    }
    Ok(PartitionRpcResponse::PartitionDropped)
}

/// Serve `PartitionSnapshotFetch`: read one page of an offered file.
async fn execute_rpc_snapshot_fetch(
    storage: &Arc<WorkerStorage>,
    partition_id: OID,
    path: &str,
    offset: u64,
    len: u64,
) -> RS<PartitionRpcResponse> {
    let path = validate_snapshot_path(partition_id, path)?;
    let full = Path::new(storage.relation_path()).join(&path);
    let fs = mudu_sys::default_sys_io_context().provider_arc().fs_arc();
    if !fs.path_exists(&full).await? {
        return Err(mudu_error!(
            ErrorCode::ObjectNotFound,
            format!("partition snapshot file {} not found", full.display())
        ));
    }
    let file_len = fs.metadata_len(&full).await?;
    if offset >= file_len {
        return Ok(PartitionRpcResponse::PartitionSnapshotFetch(Vec::new()));
    }
    let read_len = len.min(SNAPSHOT_FETCH_PAGE).min(file_len - offset);
    let file = fs.open(&full, FileOptions::read_only()).await?;
    let data = file.read_exact_at(offset, read_len as usize).await?;
    Ok(PartitionRpcResponse::PartitionSnapshotFetch(data))
}

/// The tables bound to a partition rule: `(table_id, binding)` pairs.
async fn tables_of_rule(
    meta_mgr: &Arc<dyn MetaMgr>,
    rule_id: OID,
) -> RS<
    Vec<(
        OID,
        ::catalog::contract::partition_rule_binding::TablePartitionBinding,
    )>,
> {
    let mut out = Vec::new();
    for schema in meta_mgr.list_schemas().await? {
        if let Some(binding) = meta_mgr.get_table_partition_binding(schema.id()).await? {
            if binding.rule_id == rule_id {
                out.push((schema.id(), binding));
            }
        }
    }
    Ok(out)
}

/// Serve `MigrationFenceQuery`: whether this node's routing overlay
/// currently fences the partition, and whether any in-doubt 2PC prepare
/// on this worker still touches it.
async fn execute_rpc_migration_fence_query(
    cluster_router: &Option<Arc<dyn ClusterRouter>>,
    meta_mgr: &Arc<dyn MetaMgr>,
    storage: &Arc<WorkerStorage>,
    migration_inflight: &Arc<SMutex<std::collections::HashMap<OID, usize>>>,
    partition_id: OID,
) -> RS<PartitionRpcResponse> {
    let fenced = match cluster_router {
        Some(router) => match meta_mgr.find_partition_names(partition_id).await? {
            Some((rule, partition)) => matches!(
                router.migration_state(&rule, &partition),
                ::catalog::contract::cluster_router::MigrationState::Migrating { .. }
            ),
            None => false,
        },
        None => false,
    };
    let prepared_pending = storage.participant_pending(partition_id)?;
    let inflight = migration_inflight_count(migration_inflight, partition_id) > 0;
    Ok(PartitionRpcResponse::MigrationFence {
        fenced,
        prepared_pending,
        inflight,
    })
}

impl WorkerXContract {
    /// Dispatches one migration-control RPC of the partition service.
    pub(super) async fn execute_migration_rpc(
        &self,
        request: PartitionRpcRequest,
    ) -> RS<PartitionRpcResponse> {
        match request {
            PartitionRpcRequest::PartitionSnapshotOffer { partition_id } => {
                execute_rpc_snapshot_offer(&self.storage, self.worker_id, partition_id).await
            }
            PartitionRpcRequest::PartitionSnapshotFetch {
                partition_id,
                path,
                offset,
                len,
            } => execute_rpc_snapshot_fetch(&self.storage, partition_id, &path, offset, len).await,
            PartitionRpcRequest::PartitionAdopt {
                partition_id,
                source_worker,
                register_only,
            } => {
                self.execute_rpc_partition_adopt(partition_id, source_worker, register_only)
                    .await
            }
            PartitionRpcRequest::PartitionDrop { partition_id } => {
                execute_rpc_partition_drop(
                    &self.storage,
                    self.worker_id,
                    self.partition_adoption(),
                    partition_id,
                )
                .await
            }
            PartitionRpcRequest::MigrationFenceQuery { partition_id } => {
                execute_rpc_migration_fence_query(
                    &self.cluster_router,
                    &self.meta_mgr,
                    &self.storage,
                    &self.migration_inflight,
                    partition_id,
                )
                .await
            }
            PartitionRpcRequest::PartitionSplitData {
                partition_id,
                child_partition_id,
                rule_id,
                at,
            } => {
                self.execute_rpc_split_data(partition_id, child_partition_id, rule_id, &at)
                    .await
            }
            PartitionRpcRequest::PartitionMergeData {
                left_partition_id,
                partition_id,
                rule_id,
            } => {
                self.execute_rpc_merge_data(left_partition_id, partition_id, rule_id)
                    .await
            }
            _ => Err(mudu_error!(
                ErrorCode::Internal,
                "not a migration partition rpc"
            )),
        }
    }

    /// Serve `PartitionAdopt`: pull the partition's snapshot from
    /// `source_worker`, install the files into this node's data dir, and
    /// register this worker as the partition's owner.
    async fn execute_rpc_partition_adopt(
        &self,
        partition_id: OID,
        source_worker: OID,
        register_only: bool,
    ) -> RS<PartitionRpcResponse> {
        debug!(
            worker_id = self.worker_id,
            partition_id, source_worker, register_only, "execute partition rpc adopt"
        );
        if register_only {
            let adoption = self.partition_adoption().ok_or_else(|| {
                mudu_error!(
                    ErrorCode::InvalidState,
                    "partition adoption hook is not installed on this worker"
                )
            })?;
            adoption.adopt(partition_id)?;
            return Ok(PartitionRpcResponse::PartitionAdopted(0));
        }
        let files = self
            .remote_snapshot_offer(source_worker, partition_id)
            .await?;
        let mut installed = 0;
        for (path, len, crc) in files {
            self.pull_snapshot_file(source_worker, partition_id, &path, len, crc)
                .await?;
            installed += 1;
        }
        // Reopen from the installed files on first access (drops a stale
        // in-memory copy on re-adoption).
        self.storage
            .evict_partition_relations_async(partition_id)
            .await?;
        let adoption = self.partition_adoption().ok_or_else(|| {
            mudu_error!(
                ErrorCode::InvalidState,
                "partition adoption hook is not installed on this worker"
            )
        })?;
        adoption.adopt(partition_id)?;
        Ok(PartitionRpcResponse::PartitionAdopted(installed))
    }

    /// Pulls one snapshot file from the source worker, writes it fsynced
    /// into the local data dir, and verifies its CRC64.
    async fn pull_snapshot_file(
        &self,
        source_worker: OID,
        partition_id: OID,
        path: &str,
        len: u64,
        crc: u64,
    ) -> RS<()> {
        let full = Path::new(self.storage.relation_path()).join(path);
        let fs = mudu_sys::default_sys_io_context().provider_arc().fs_arc();
        if let Some(parent) = full.parent() {
            fs.create_dir_all(parent).await?;
        }
        let file = fs.open(&full, FileOptions::read_write_create()).await?;
        let mut offset = 0u64;
        while offset < len {
            let page = self
                .remote_snapshot_fetch(
                    source_worker,
                    partition_id,
                    path,
                    offset,
                    SNAPSHOT_FETCH_PAGE.min(len - offset),
                )
                .await?;
            if page.is_empty() {
                return Err(mudu_error!(
                    ErrorCode::Network,
                    format!("snapshot fetch of {path} ended early at {offset} of {len}")
                ));
            }
            file.write_all_at(offset, &page).await?;
            offset += page.len() as u64;
        }
        file.fsync().await?;
        drop(file);
        let written = fs.read_all(&full).await?;
        if crc64(&written) != crc {
            return Err(mudu_error!(
                ErrorCode::CorruptedData,
                format!("adopted snapshot file {path} failed its CRC64 check")
            ));
        }
        Ok(())
    }
}
impl WorkerXContract {
    /// Serve `PartitionSplitData`: re-home the parent's rows at/above the
    /// split point into the child — one local transaction per bound table
    /// (runs on the fenced owner worker; storage-level writes bypass the
    /// RPC/statement fences by design, the window is the driver's fence).
    async fn execute_rpc_split_data(
        &self,
        parent: OID,
        child: OID,
        rule_id: OID,
        at: &[String],
    ) -> RS<PartitionRpcResponse> {
        let mut rehomed = 0usize;
        for (table_id, binding) in tables_of_rule(&self.meta_mgr, rule_id).await? {
            let desc = self.meta_mgr.get_table_by_id(table_id).await?;
            // The bound tuple typed by THIS table's route key (the catalog
            // stores textual bounds; runtime routing encodes per table).
            let route_desc = ::xengine::partition_router::build_route_tuple_desc(
                &desc,
                &binding.ref_attr_indices,
            )?;
            let at_text: Vec<Vec<u8>> = at.iter().map(|v| v.clone().into_bytes()).collect();
            let at_tuple =
                ::xengine::partition_router::build_partition_bound_tuple(&route_desc, &at_text)?;
            let key_desc = desc.key_desc().clone();
            let route_width = route_desc.field_count();
            // Re-home under the exclusive fence reads at the live edge:
            // versions committed just before the fence went up are
            // provably committed and must be visible to the move.
            let tx = worker_begin_tx_reading_current(&self.snapshot_mgr)?;
            let result = async {
                let rows = self
                    .storage
                    .range_on_partition(
                        table_id,
                        Some(parent),
                        (std::ops::Bound::Unbounded, std::ops::Bound::Unbounded),
                        tx.as_ref(),
                    )
                    .await?;
                for (key, value) in rows {
                    // Project the stored key to the route columns (the
                    // partition key is the leading prefix of the table key,
                    // validated by the driver).
                    let key_ref = mudu_contract::tuple::tuple_ref::TupleRef::new(&key, &key_desc);
                    let mut route_cols = Vec::with_capacity(route_width);
                    for field in 0..route_width {
                        route_cols.push(key_ref.get_binary_data(field)?.to_vec());
                    }
                    let route_tuple =
                        mudu_contract::tuple::build_tuple::build_tuple(&route_cols, &route_desc)?;
                    // Rows with a route key at/above the split point belong
                    // to the child.
                    if !mudu_contract::tuple::comparator::tuple_compare(
                        &route_desc,
                        &route_tuple,
                        &at_tuple,
                    )?
                    .is_lt()
                    {
                        self.storage
                            .put_on_partition(
                                table_id,
                                Some(child),
                                key.clone(),
                                value.clone(),
                                tx.as_ref(),
                            )
                            .await?;
                        self.storage
                            .remove_on_partition(table_id, Some(parent), &key, tx.as_ref())
                            .await?;
                        rehomed += 1;
                    }
                }
                Ok::<(), mudu::error::MuduError>(())
            }
            .await;
            match result {
                Ok(()) => {
                    // The versions stamp with the tx's joined epoch; the
                    // driver's flip follows this response, so the commit
                    // must be stable first (stable-epoch snapshot reads).
                    let epoch = tx.snapshot().epoch();
                    let _ = self.worker_commit_tx_async(tx).await?;
                    if let Some(epoch) = epoch {
                        if let Some(oracle) = self.snapshot_mgr.epoch_oracle() {
                            oracle.wait_stable(epoch).await?;
                        }
                    }
                }
                Err(err) => {
                    worker_rollback_tx(&self.tx_lock, &self.snapshot_mgr, tx)?;
                    return Err(err);
                }
            }
        }
        Ok(PartitionRpcResponse::PartitionRehomed(rehomed))
    }

    /// Serve `PartitionMergeData`: move every row of the right partition
    /// into the left — one local transaction per bound table.
    async fn execute_rpc_merge_data(
        &self,
        left: OID,
        right: OID,
        rule_id: OID,
    ) -> RS<PartitionRpcResponse> {
        let mut rehomed = 0usize;
        for (table_id, _) in tables_of_rule(&self.meta_mgr, rule_id).await? {
            // Re-home under the exclusive fence reads at the live edge:
            // versions committed just before the fence went up are
            // provably committed and must be visible to the move.
            let tx = worker_begin_tx_reading_current(&self.snapshot_mgr)?;
            let result = async {
                let rows = self
                    .storage
                    .range_on_partition(
                        table_id,
                        Some(right),
                        (std::ops::Bound::Unbounded, std::ops::Bound::Unbounded),
                        tx.as_ref(),
                    )
                    .await?;
                for (key, value) in rows {
                    self.storage
                        .put_on_partition(
                            table_id,
                            Some(left),
                            key.clone(),
                            value.clone(),
                            tx.as_ref(),
                        )
                        .await?;
                    self.storage
                        .remove_on_partition(table_id, Some(right), &key, tx.as_ref())
                        .await?;
                    rehomed += 1;
                }
                Ok::<(), mudu::error::MuduError>(())
            }
            .await;
            match result {
                Ok(()) => {
                    let epoch = tx.snapshot().epoch();
                    let _ = self.worker_commit_tx_async(tx).await?;
                    if let Some(epoch) = epoch {
                        if let Some(oracle) = self.snapshot_mgr.epoch_oracle() {
                            oracle.wait_stable(epoch).await?;
                        }
                    }
                }
                Err(err) => {
                    worker_rollback_tx(&self.tx_lock, &self.snapshot_mgr, tx)?;
                    return Err(err);
                }
            }
        }
        Ok(PartitionRpcResponse::PartitionRehomed(rehomed))
    }
}
