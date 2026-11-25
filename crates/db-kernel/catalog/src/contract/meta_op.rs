//! The replicated metadata operation log of the unified control plane:
//! every catalog DDL travels the control group's catalog stream so every
//! node applies the identical change.
//!
//! This module carries only the pure op type and the service contract; the
//! stream-replicated implementation lives in `repl::control_plane`.

use crate::contract::fs_type::{FsTypeDesc, FsTypeKind};
use crate::contract::partition_rule::PartitionRuleDesc;
use crate::contract::partition_rule_binding::{PartitionPlacement, TablePartitionBinding};
use crate::contract::schema_class::SchemaClass;
use async_trait::async_trait;
use mudu::common::id::OID;
use mudu::common::result::RS;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// One catalog mutation replicated through the control plane's catalog stream.
///
/// Carrying the structured op (not SQL text) keeps the log
/// versioned alongside the catalog types.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum MetaOp {
    /// `CREATE TABLE` (with its partition binding, when any).
    CreateTable {
        /// Table schema (derived oid).
        schema: SchemaClass,
        /// Partition binding, when the table is partitioned.
        binding: Option<TablePartitionBinding>,
    },
    /// `DROP TABLE`.
    DropTable {
        /// Oid of the dropped table.
        oid: OID,
    },
    /// `CREATE QUEUE`.
    CreateQueue {
        /// Queue schema (derived oid, `SchemaClassKind::Queue`).
        schema: SchemaClass,
        /// Partition binding, when the queue is partitioned.
        binding: Option<TablePartitionBinding>,
    },
    /// `DROP QUEUE`.
    DropQueue {
        /// Oid of the dropped queue.
        oid: OID,
    },
    /// `CREATE PARTITION RULE`.
    CreatePartitionRule {
        /// The rule descriptor (derived oids).
        rule: PartitionRuleDesc,
    },
    /// `CREATE PARTITION PLACEMENT` (upsert of partition → worker).
    UpsertPartitionPlacements {
        /// The placements to upsert.
        placements: Vec<PartitionPlacement>,
    },
    /// `CREATE TYPE` (filesystem type).
    CreateFsType {
        /// Name of the filesystem type.
        name: String,
        /// Kind of the filesystem type.
        kind: FsTypeKind,
        /// Allocated fs id.
        fs_id: u64,
    },
    /// `DROP TYPE`.
    DropFsType {
        /// Name of the dropped filesystem type.
        name: String,
    },
    /// Begins moving one partition to another node group (phase 3): every
    /// node marks the partition `Migrating` in its routing overlay; the
    /// source owner fences new writes to it until the migration completes
    /// or aborts. Catalog content (rule, binding) is unchanged.
    BeginMigration {
        /// Name of the partition rule.
        rule: String,
        /// Name of the partition within the rule.
        partition: String,
        /// Oid of the partition (name-derived).
        partition_id: OID,
        /// Target node group.
        target_group: u64,
        /// Target worker index within the group's nodes.
        target_worker_index: u32,
        /// Whether the fence also covers reads (splits/merges re-home rows
        /// in place; moves keep the source readable during the window).
        fence_reads: bool,
    },
    /// Completes a begun migration: the placement flips to the target
    /// atomically (at this entry's raft index, which is also the placement
    /// version) on every node.
    CompleteMigration {
        /// Name of the partition rule.
        rule: String,
        /// Name of the partition within the rule.
        partition: String,
    },
    /// Aborts a begun migration: the partition stays on the source and the
    /// write fence lifts.
    AbortMigration {
        /// Name of the partition rule.
        rule: String,
        /// Name of the partition within the rule.
        partition: String,
    },
    /// Splits one range partition at `at` into itself plus a new partition;
    /// the child inherits the parent's effective placement (fenced while
    /// the split redistributes rows). The rule's version bumps on apply.
    SplitPartition {
        /// Name of the partition rule.
        rule: String,
        /// Name of the parent partition.
        partition: String,
        /// Split point (textual bound values, one per partition-key column).
        at: Vec<String>,
        /// Name of the new partition (the `[at, end)` range).
        new_partition: String,
    },
    /// Merges two adjacent partitions: `left` absorbs `right`'s range and
    /// `right` is dropped. The rule's version bumps on apply.
    MergePartition {
        /// Name of the partition rule.
        rule: String,
        /// Name of the surviving (left) partition.
        left: String,
        /// Name of the dropped (right) partition.
        right: String,
    },
}

/// The handle of a node's control-plane service, consumed by the DDL command
/// executors.
#[async_trait]
pub trait MetaGroupApi: Send + Sync {
    /// Proposes `op` to the control plane and returns once it is committed **and
    /// applied locally** (so the issuing session sees its own DDL).
    async fn apply(&self, op: MetaOp) -> RS<()>;

    /// Whether the control-plane service is running on this node.
    fn is_running(&self) -> bool;
}

/// Shared slot for the node's control-plane handle, installed by the server at
/// startup (same late-binding pattern as the raft share).
pub type MetaGroupSlot = Arc<mudu_sys::sync::SMutex<Option<Arc<dyn MetaGroupApi>>>>;

/// Creates an empty control-plane slot.
pub fn new_meta_group_slot() -> MetaGroupSlot {
    Arc::new(mudu_sys::sync::SMutex::new(None))
}

/// A no-op handle for tests and non-cluster deployments.
pub struct NoopMetaGroup;

/// Client RPC of the control plane: forward a proposal to the control leader.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum MetaGroupRpcRequest {
    /// Propose `op`; `request_id` deduplicates retries. The metadata payload is boxed.
    Propose {
        /// Idempotency key of the proposal.
        request_id: u128,
        /// The metadata operation.
        op: Box<MetaOp>,
    },
}

/// Response of a control-plane client RPC.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum MetaGroupRpcResponse {
    /// The operation committed and was applied locally.
    Applied,
    /// The responder is not the control leader; retry at `leader` when known.
    /// `term` is the responder's current consensus term: hint caches must
    /// never let an older-term hint overwrite a newer one.
    NotLeader {
        /// The current control leader's node id, when known.
        leader: Option<u64>,
        /// The responder's current term.
        term: u64,
    },
    /// The operation failed; carries the error's display string.
    Err(String),
}

mudu::impl_channel_message!(
    MetaGroupRpcRequest,
    reply = MetaGroupRpcResponse,
    kind = crate::contract::channel_kinds::CLUSTER_META_RPC
);

/// A forwarded proposal, sent fire-and-forget to the meta leader by a node
/// whose local handler was asked by a client (the outcome flows back through
/// the replicated log and the asker's local apply).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MetaGroupForward {
    /// Idempotency key of the proposal (travels into the log entry so the
    /// originator's local apply resolves its waiter).
    pub request_id: u128,
    /// The metadata operation.
    pub op: MetaOp,
}

mudu::impl_channel_message!(
    MetaGroupForward,
    tell_only,
    kind = crate::contract::channel_kinds::CLUSTER_META_FORWARD
);
#[async_trait]
impl MetaGroupApi for NoopMetaGroup {
    async fn apply(&self, _op: MetaOp) -> RS<()> {
        Err(mudu::mudu_error!(
            mudu::error::ErrorCode::NotImplemented,
            "no meta group configured"
        ))
    }

    fn is_running(&self) -> bool {
        false
    }
}

/// Describes a `FsTypeDesc` for `CreateFsType` ops (helper for the command
/// layer, which allocates the fs id).
pub fn fs_type_desc(name: &str, kind: FsTypeKind, fs_id: u64) -> FsTypeDesc {
    FsTypeDesc::new(name.to_string(), fs_id, kind)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::contract::fs_type::FsColumnBinding;
    use mudu::error::ErrorCode;

    #[test]
    fn unconfigured_meta_group_rejects_writes_and_slot_can_be_installed() {
        let slot = new_meta_group_slot();
        assert!(slot.lock().unwrap().is_none());
        let group: Arc<dyn MetaGroupApi> = Arc::new(NoopMetaGroup);
        assert!(!group.is_running());
        let op = MetaOp::DropTable { oid: 42 };
        let error = futures::executor::block_on(group.apply(op)).unwrap_err();
        assert_eq!(error.ec(), ErrorCode::NotImplemented);
        *slot.lock().unwrap() = Some(group.clone());
        assert!(Arc::ptr_eq(slot.lock().unwrap().as_ref().unwrap(), &group));
    }

    #[test]
    fn filesystem_metadata_preserves_kind_identity_and_rejects_unknown_codes() {
        for kind in [FsTypeKind::File, FsTypeKind::Directory] {
            let desc = fs_type_desc("custom", kind, u64::MAX);
            assert_eq!(desc.name(), "custom");
            assert_eq!(desc.fs_id(), u64::MAX);
            assert_eq!(desc.kind(), kind);
            assert_eq!(FsTypeKind::from_u32(kind.as_u32()).unwrap(), kind);
            let binding = FsColumnBinding::new(desc.fs_id(), desc.kind());
            assert_eq!(binding.fs_id(), u64::MAX);
            assert_eq!(binding.kind(), kind);
        }
        for invalid in [0, 3, u32::MAX] {
            assert_eq!(
                FsTypeKind::from_u32(invalid).unwrap_err().ec(),
                ErrorCode::InvalidArgument
            );
        }
    }
}
