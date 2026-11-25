//! Split/merge bodies of the migration driver, kept in a sibling module for
//! the code-metrics limits.

use catalog::contract::cluster_router::{ClusterRouter, MigrationState};
use catalog::contract::derive_oid::derive_partition_oid;
use catalog::contract::meta_op::MetaOp;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use tracing::{debug, warn};
use xcontract::partition_rpc::{PartitionRpcRequest, PartitionRpcResponse};

use crate::backend::http_api::{PartitionMergeRequest, PartitionSplitRequest};

use super::migration_driver::{
    MoveOutcome, SplitMergePlan, parent_group_of, partition_def, worker_index_in_group,
};
use catalog::contract::derive_oid::derive_worker_oid;

impl super::migration_driver::MigrationDriver {
    /// Splits one partition at `at` into itself plus a new partition:
    /// fence the parent (reads included), replicate the catalog change,
    /// re-home rows on the owner, then lift the fences and register the
    /// child on every member of the group.
    pub(crate) async fn split_partition(&self, request: PartitionSplitRequest) -> RS<MoveOutcome> {
        let rule_desc = self.rule_by_name(&request.rule).await?;
        let parent = partition_def(&rule_desc, &request.partition)?;
        self.reject_queue_bindings(rule_desc.oid).await?;
        self.validate_split_key_prefix(rule_desc.oid).await?;
        let parent_placement = self
            .directory
            .placement(&request.rule, &request.partition)
            .ok_or_else(|| {
                mudu_error!(
                    ErrorCode::ObjectNotFound,
                    format!(
                        "partition {}.{} has no cluster placement",
                        request.rule, request.partition
                    )
                )
            })?;
        if rule_desc
            .partitions
            .iter()
            .any(|def| def.name == request.new_partition)
        {
            return Err(mudu_error!(
                ErrorCode::ObjectAlreadyExists,
                format!(
                    "partition rule {} already has partition {}",
                    request.rule, request.new_partition
                )
            ));
        }
        if matches!(
            self.directory
                .migration_state(&request.rule, &request.partition),
            MigrationState::Migrating { .. }
        ) {
            return Err(mudu_error!(
                ErrorCode::ResourceBusy,
                format!(
                    "partition {}.{} is migrating; abort or wait first",
                    request.rule, request.partition
                )
            ));
        }
        // Pre-validate the transform on this catalog copy.
        catalog::contract::partition_rule::split_rule_partitions(
            &rule_desc,
            &request.partition,
            request.at.iter().map(|v| v.clone().into_bytes()).collect(),
            &request.new_partition,
        )?;
        let child_partition_id = derive_partition_oid(&request.rule, &request.new_partition);
        let owner = self.group_worker_endpoint(parent_placement.0, parent_placement.1)?;
        let split_plan = SplitMergePlan {
            rule_id: rule_desc.oid,
            primary_partition_id: parent.partition_id,
            secondary_partition_id: child_partition_id,
            owner,
        };

        // Fence the parent (reads included: the child range reads would
        // miss rows until the redistribution commits).
        self.meta_group
            .apply(MetaOp::BeginMigration {
                rule: request.rule.clone(),
                partition: request.partition.clone(),
                partition_id: parent.partition_id,
                target_group: parent_placement.0,
                target_worker_index: parent_placement.1,
                fence_reads: true,
            })
            .await?;
        let split_result = self.run_split_body(&request, &split_plan).await;
        if let Err(err) = &split_result {
            // Roll back the fence: the split never happened.
            let _ = self
                .meta_group
                .apply(MetaOp::AbortMigration {
                    rule: request.rule.clone(),
                    partition: request.partition.clone(),
                })
                .await;
            return Err(mudu_error!(err.ec(), "partition split failed", err.clone()));
        }
        split_result
    }

    /// Merges two adjacent partitions of one rule: both must share one
    /// placement. Fences both, replicates the catalog change, re-homes the
    /// right partition's rows into the left, drops the right.
    pub(crate) async fn merge_partitions(&self, request: PartitionMergeRequest) -> RS<MoveOutcome> {
        let rule_desc = self.rule_by_name(&request.rule).await?;
        let left = partition_def(&rule_desc, &request.left)?;
        let right = partition_def(&rule_desc, &request.right)?;
        self.reject_queue_bindings(rule_desc.oid).await?;
        let (owner, left_placement) = self.validate_merge(&rule_desc, &request).await?;
        // Pre-validate the transform.
        catalog::contract::partition_rule::merge_rule_partitions(
            &rule_desc,
            &request.left,
            &request.right,
        )?;
        let merge_plan = SplitMergePlan {
            rule_id: rule_desc.oid,
            primary_partition_id: left.partition_id,
            secondary_partition_id: right.partition_id,
            owner,
        };

        // Fence both partitions (reads included).
        for (partition, partition_id) in [
            (&request.left, left.partition_id),
            (&request.right, right.partition_id),
        ] {
            self.meta_group
                .apply(MetaOp::BeginMigration {
                    rule: request.rule.clone(),
                    partition: partition.clone(),
                    partition_id,
                    target_group: left_placement.0,
                    target_worker_index: left_placement.1,
                    fence_reads: true,
                })
                .await?;
        }
        let merge_result = self.run_merge_body(&request, &merge_plan).await;
        if let Err(err) = &merge_result {
            for partition in [&request.left, &request.right] {
                let _ = self
                    .meta_group
                    .apply(MetaOp::AbortMigration {
                        rule: request.rule.clone(),
                        partition: partition.clone(),
                    })
                    .await;
            }
            return Err(mudu_error!(err.ec(), "partition merge failed", err.clone()));
        }
        merge_result
    }
    /// The fenced split body: catalog change → redistribution → lift both
    /// fences → register the child on every member of the group.
    pub(crate) async fn run_split_body(
        &self,
        request: &PartitionSplitRequest,
        plan: &SplitMergePlan,
    ) -> RS<MoveOutcome> {
        self.confirm_fence(plan.owner, plan.primary_partition_id)
            .await?;
        self.meta_group
            .apply(MetaOp::SplitPartition {
                rule: request.rule.clone(),
                partition: request.partition.clone(),
                at: request.at.clone(),
                new_partition: request.new_partition.clone(),
            })
            .await?;
        let rehomed = match self
            .ask_worker_rpc(
                plan.owner,
                PartitionRpcRequest::PartitionSplitData {
                    partition_id: plan.primary_partition_id,
                    child_partition_id: plan.secondary_partition_id,
                    rule_id: plan.rule_id,
                    at: request.at.clone(),
                },
            )
            .await?
        {
            PartitionRpcResponse::PartitionRehomed(rows) => rows,
            PartitionRpcResponse::Err(err) => {
                return Err(mudu_error!(ErrorCode::Internal, err));
            }
            _ => {
                return Err(mudu_error!(
                    ErrorCode::Internal,
                    "unexpected split data rpc response"
                ));
            }
        };
        // Lift the parent's fence (its placement is unchanged) and flip the
        // child's fence to its inherited placement.
        self.meta_group
            .apply(MetaOp::CompleteMigration {
                rule: request.rule.clone(),
                partition: request.partition.clone(),
            })
            .await?;
        self.meta_group
            .apply(MetaOp::CompleteMigration {
                rule: request.rule.clone(),
                partition: request.new_partition.clone(),
            })
            .await?;
        // Register the child on every member of the group (replicas get the
        // rows through the group's WAL stream; the marker keeps snapshots
        // and restarts correct).
        self.register_on_group_members(
            parent_group_of(&self.directory, plan.owner)?,
            plan.secondary_partition_id,
            plan.owner,
        )
        .await?;
        debug!(
            rule = %request.rule,
            partition = %request.partition,
            new_partition = %request.new_partition,
            rehomed,
            "partition split completed"
        );
        Ok(MoveOutcome {
            partition_id: plan.secondary_partition_id,
            group_id: parent_group_of(&self.directory, plan.owner)?,
            node_id: None,
            client_addr: None,
            placement_version: self.directory.placement_version(),
            rows_rehomed: rehomed as u64,
        })
    }

    /// The fenced merge body: catalog change → redistribution → lift the
    /// left fence and clear the right's entry → drop the right's data on
    /// every member of the group.
    pub(crate) async fn run_merge_body(
        &self,
        request: &PartitionMergeRequest,
        plan: &SplitMergePlan,
    ) -> RS<MoveOutcome> {
        self.confirm_fence(plan.owner, plan.primary_partition_id)
            .await?;
        self.confirm_fence(plan.owner, plan.secondary_partition_id)
            .await?;
        self.meta_group
            .apply(MetaOp::MergePartition {
                rule: request.rule.clone(),
                left: request.left.clone(),
                right: request.right.clone(),
            })
            .await?;
        let rehomed = match self
            .ask_worker_rpc(
                plan.owner,
                PartitionRpcRequest::PartitionMergeData {
                    left_partition_id: plan.primary_partition_id,
                    partition_id: plan.secondary_partition_id,
                    rule_id: plan.rule_id,
                },
            )
            .await?
        {
            PartitionRpcResponse::PartitionRehomed(rows) => rows,
            PartitionRpcResponse::Err(err) => {
                return Err(mudu_error!(ErrorCode::Internal, err));
            }
            _ => {
                return Err(mudu_error!(
                    ErrorCode::Internal,
                    "unexpected merge data rpc response"
                ));
            }
        };
        self.meta_group
            .apply(MetaOp::CompleteMigration {
                rule: request.rule.clone(),
                partition: request.left.clone(),
            })
            .await?;
        // The right partition no longer exists in the rule; clear its
        // overlay entry outright.
        self.meta_group
            .apply(MetaOp::AbortMigration {
                rule: request.rule.clone(),
                partition: request.right.clone(),
            })
            .await?;
        // Drop the right partition's data files on every member (the rows
        // live in the left now).
        let group_id = parent_group_of(&self.directory, plan.owner)?;
        let group = self.directory.topology().group(group_id).cloned();
        if let Some(group) = group {
            for member in &group.members {
                let endpoint = derive_worker_oid(
                    member.node_id,
                    worker_index_in_group(&self.directory, group_id, plan.owner),
                );
                if let Err(err) = self
                    .ask_worker_rpc(
                        endpoint,
                        PartitionRpcRequest::PartitionDrop {
                            partition_id: plan.secondary_partition_id,
                        },
                    )
                    .await
                {
                    warn!(
                        partition_id = plan.secondary_partition_id,
                        node_id = member.node_id,
                        "merge cleanup of the right partition failed: {err}"
                    );
                }
            }
        }
        debug!(
            rule = %request.rule,
            left = %request.left,
            right = %request.right,
            rehomed,
            "partition merge completed"
        );
        Ok(MoveOutcome {
            partition_id: plan.primary_partition_id,
            group_id,
            node_id: self.directory.serving_node(group_id),
            client_addr: None,
            placement_version: self.directory.placement_version(),
            rows_rehomed: rehomed as u64,
        })
    }

    /// Merge validation: adjacency in the rule's order, one shared
    /// placement, no in-flight migration; returns the owner endpoint.
    async fn validate_merge(
        &self,
        rule_desc: &catalog::contract::partition_rule::PartitionRuleDesc,
        request: &PartitionMergeRequest,
    ) -> RS<(OID, (u64, u32))> {
        let positions = |name: &str| rule_desc.partitions.iter().position(|def| def.name == name);
        let (Some(left_index), Some(right_index)) =
            (positions(&request.left), positions(&request.right))
        else {
            return Err(mudu_error!(ErrorCode::Internal, "partition positions lost"));
        };
        if right_index != left_index + 1 {
            return Err(mudu_error!(
                ErrorCode::InvalidArgument,
                format!(
                    "partitions {} and {} of rule {} are not adjacent",
                    request.left, request.right, request.rule
                )
            ));
        }
        let left_placement = self
            .directory
            .placement(&request.rule, &request.left)
            .ok_or_else(|| {
                mudu_error!(
                    ErrorCode::ObjectNotFound,
                    format!(
                        "partition {}.{} has no cluster placement",
                        request.rule, request.left
                    )
                )
            })?;
        let right_placement = self
            .directory
            .placement(&request.rule, &request.right)
            .ok_or_else(|| {
                mudu_error!(
                    ErrorCode::ObjectNotFound,
                    format!(
                        "partition {}.{} has no cluster placement",
                        request.rule, request.right
                    )
                )
            })?;
        if left_placement != right_placement {
            return Err(mudu_error!(
                ErrorCode::InvalidArgument,
                "merge requires both partitions on the same group and worker; move one first"
            ));
        }
        for partition in [&request.left, &request.right] {
            if matches!(
                self.directory.migration_state(&request.rule, partition),
                MigrationState::Migrating { .. }
            ) {
                return Err(mudu_error!(
                    ErrorCode::ResourceBusy,
                    format!("partition {}.{partition} is migrating", request.rule)
                ));
            }
        }
        let owner = self.group_worker_endpoint(left_placement.0, left_placement.1)?;
        Ok((owner, left_placement))
    }
}
