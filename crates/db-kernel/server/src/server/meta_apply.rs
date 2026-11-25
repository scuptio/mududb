//! The node-level catalog applier of the control plane: applies committed
//! catalog operations to the node's meta manager and the placement overlay
//! of the cluster directory.
//!
//! The op version is the op's control-stream LSN: it orders
//! placement-affecting ops for the routing overlay (replacing the old
//! meta-group raft index as the placement version).

use std::sync::Arc;

use ::catalog::contract::meta_op::MetaOp;
use async_trait::async_trait;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;

use super::cluster_directory::ClusterDirectory;

/// The node-level catalog applier: applies committed meta ops to the node's
/// meta manager (relations open lazily on first access) and the placement
/// overlay of the cluster directory. The meta manager is created on first
/// use, inside the dispatch task's runtime.
pub(crate) struct NodeMetaApplier {
    data_dir: String,
    meta_mgr: mudu_sys::sync::SMutex<Option<Arc<dyn catalog::contract::meta_mgr::MetaMgr>>>,
    directory: Arc<ClusterDirectory>,
}

impl NodeMetaApplier {
    /// Creates the applier; the meta manager opens lazily on the first op.
    pub(crate) fn new(data_dir: &str, directory: Arc<ClusterDirectory>) -> Self {
        Self {
            data_dir: data_dir.to_string(),
            meta_mgr: mudu_sys::sync::SMutex::new(None),
            directory,
        }
    }
}

#[async_trait]
impl ::repl::control_plane::MetaApplier for NodeMetaApplier {
    async fn validate_recovery(
        &self,
        log: &repl::log_stream::DurableStreamLog,
        applied: Option<&mudu_raft::stream::StreamProgress>,
        committed: Option<&mudu_raft::stream::StreamProgress>,
    ) -> RS<()> {
        self.directory
            .validate_control_recovery(log, applied, committed)
            .await
    }

    async fn apply(&self, op: MetaOp, version: u64) -> RS<()> {
        // Placement-affecting ops drive the routing overlay (persisted by
        // the directory); split/merge additionally transform the catalog
        // (the child's overlay entry AND the rule's new ranges).
        let placement = self.directory.apply_placement_op(&op, version).await?;
        let catalog_op = matches!(
            op,
            MetaOp::SplitPartition { .. } | MetaOp::MergePartition { .. }
        );
        if placement && !catalog_op {
            return Ok(());
        }
        let meta_mgr = {
            let slot = self
                .meta_mgr
                .lock()
                .map_err(|_| mudu_error!(ErrorCode::Internal, "meta applier lock poisoned"))?;
            slot.as_ref().cloned()
        };
        let meta_mgr = match meta_mgr {
            Some(meta_mgr) => meta_mgr,
            None => {
                let created =
                    ::meta::meta_mgr_factory::MetaMgrFactory::create(self.data_dir.clone()).await?;
                created.initialize().await?;
                let mut slot = self
                    .meta_mgr
                    .lock()
                    .map_err(|_| mudu_error!(ErrorCode::Internal, "meta applier lock poisoned"))?;
                match slot.as_ref() {
                    Some(existing) => existing.clone(),
                    None => {
                        *slot = Some(created.clone());
                        created
                    }
                }
            }
        };
        apply_meta_op(meta_mgr.as_ref(), op).await?;
        // The control stream persists its applied frontier only after this
        // returns. Catalog pages must survive independently of that frontier.
        meta_mgr.flush_dirty_pages().await
    }
}

/// Applies one committed meta op to the node's catalog; idempotent so stream
/// replay after a restart is safe.
async fn apply_meta_op(meta_mgr: &dyn catalog::contract::meta_mgr::MetaMgr, op: MetaOp) -> RS<()> {
    match op {
        MetaOp::CreateTable { schema, binding } => {
            verify_binding(meta_mgr, &schema, binding.as_ref()).await?;
            ensure_object(meta_mgr, &schema, false).await?;
            ensure_binding(meta_mgr, binding.as_ref()).await
        }
        MetaOp::DropTable { oid } => match meta_mgr.get_table_by_id(oid).await {
            Ok(_) => meta_mgr.drop_table(oid).await,
            Err(error) if error.ec() == ErrorCode::ObjectNotFound => {
                cleanup_orphan_binding(meta_mgr, oid).await
            }
            Err(error) => Err(error),
        },
        MetaOp::CreateQueue { schema, binding } => {
            // Validate both definitions before creating either object. Missing
            // substeps remain resumable, but conflicts must not create a queue.
            verify_existing_object(meta_mgr, &schema, true).await?;
            let dlq = ::catalog::contract::queue::queue_dlq_schema(&schema);
            verify_existing_object(meta_mgr, &dlq, true).await?;
            verify_binding(meta_mgr, &schema, binding.as_ref()).await?;
            ensure_object(meta_mgr, &schema, true).await?;
            ensure_object(meta_mgr, &dlq, true).await?;
            ensure_binding(meta_mgr, binding.as_ref()).await
        }
        MetaOp::DropQueue { oid } => replay::drop_queue(meta_mgr, oid).await,
        MetaOp::CreatePartitionRule { rule } => {
            match meta_mgr.get_partition_rule_by_id(rule.oid).await {
                Ok(existing) if existing == rule => Ok(()),
                Ok(_) => Err(mudu_error!(
                    ErrorCode::InvalidState,
                    "replayed partition rule conflicts with catalog"
                )),
                Err(error) if error.ec() == ErrorCode::ObjectNotFound => {
                    meta_mgr.create_partition_rule(&rule).await
                }
                Err(error) => Err(error),
            }
        }
        MetaOp::UpsertPartitionPlacements { placements } => {
            meta_mgr.upsert_partition_placements(&placements).await
        }
        MetaOp::CreateFsType { name, kind, fs_id } => {
            if fs_id != catalog::contract::derive_oid::derive_fs_id(&name) {
                return Err(mudu_error!(
                    ErrorCode::InvalidState,
                    "filesystem type ID does not match its name"
                ));
            }
            if let Some(existing) = meta_mgr.get_fs_type_by_name(&name).await? {
                return if existing.fs_id() == fs_id && existing.kind() == kind {
                    Ok(())
                } else {
                    Err(mudu_error!(
                        ErrorCode::InvalidState,
                        "replayed filesystem type conflicts with catalog"
                    ))
                };
            }
            let allocated = meta_mgr.create_fs_type(&name, kind).await?;
            if allocated != fs_id {
                return Err(mudu_error!(
                    ErrorCode::InvalidState,
                    format!(
                        "fs type id mismatch for {}: {} != {}",
                        name, allocated, fs_id
                    )
                ));
            }
            Ok(())
        }
        MetaOp::DropFsType { name } => {
            if meta_mgr.get_fs_type_by_name(&name).await?.is_some() {
                meta_mgr.drop_fs_type(&name).await?;
            }
            Ok(())
        }
        // Placement operations are consumed by apply_placement_op.
        MetaOp::BeginMigration { .. }
        | MetaOp::CompleteMigration { .. }
        | MetaOp::AbortMigration { .. } => Ok(()),
        MetaOp::SplitPartition { .. } | MetaOp::MergePartition { .. } => {
            apply_split_merge_op(meta_mgr, op).await
        }
    }
}

/// Applies a split/merge op: transforms the rule's ranges (bumping its
/// version) and writes the catalog; idempotent so stream replay after a
/// restart is safe.
async fn apply_split_merge_op(
    meta_mgr: &dyn catalog::contract::meta_mgr::MetaMgr,
    op: MetaOp,
) -> RS<()> {
    match op {
        MetaOp::SplitPartition {
            rule,
            partition,
            at,
            new_partition,
        } => {
            let current = meta_mgr
                .get_partition_rule_by_name(&rule)
                .await?
                .ok_or_else(|| {
                    mudu_error!(
                        ErrorCode::ObjectNotFound,
                        format!("no such partition rule {rule}")
                    )
                })?;
            let at_bytes: Vec<Vec<u8>> = at.iter().map(|v| v.clone().into_bytes()).collect();
            let updated = ::catalog::contract::partition_rule::split_rule_partitions(
                &current,
                &partition,
                at_bytes,
                &new_partition,
            )?;
            if updated.version != current.version {
                meta_mgr.update_partition_rule(&updated).await?;
            }
            Ok(())
        }
        MetaOp::MergePartition { rule, left, right } => {
            let current = meta_mgr
                .get_partition_rule_by_name(&rule)
                .await?
                .ok_or_else(|| {
                    mudu_error!(
                        ErrorCode::ObjectNotFound,
                        format!("no such partition rule {rule}")
                    )
                })?;
            let updated = ::catalog::contract::partition_rule::merge_rule_partitions(
                &current, &left, &right,
            )?;
            if updated.version != current.version {
                meta_mgr.update_partition_rule(&updated).await?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

mod replay;
use replay::{
    cleanup_orphan_binding, ensure_binding, ensure_object, verify_binding, verify_existing_object,
};

#[cfg(test)]
mod tests;
