//! The migration driver: orchestrates partition moves,
//! splits, and merges — online copy, partition-local cutover quiescence
//! (fence + prepared-drain + in-flight drain), placement flip through the
//! MetaGroup, and the source cleanup. Shared by the management HTTP
//! endpoints and the rebalance scheduler.
//!
//! Every step is idempotent so a retried call resumes an interrupted
//! operation. The driver runs wherever it is constructed (management
//! request landing node, or the MetaGroup leader for the scheduler).

use std::sync::Arc;
use std::time::Duration;

use catalog::contract::cluster_router::{ClusterRouter, MigrationState};
use catalog::contract::derive_oid::{derive_partition_oid, derive_worker_oid};
use catalog::contract::meta_mgr::MetaMgr;
use catalog::contract::meta_op::{MetaGroupApi, MetaOp};
use catalog::contract::partition_rule::{PartitionRuleDesc, RangePartitionDef};
use catalog::contract::schema_class::SchemaClassKind;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use tracing::{debug, warn};
use xcontract::partition_rpc::{PartitionRpcRequest, PartitionRpcResponse};

use crate::backend::http_api::PartitionMoveRequest;
use ::server::server::cluster_directory::ClusterDirectory;

/// Timeout of one migration partition-RPC round-trip.
const MIGRATION_RPC_TIMEOUT: Duration = Duration::from_secs(30);
/// Fence-confirmation poll budget (the source owner's overlay applies the
/// `BeginMigration` record promptly after its commit).
const FENCE_CONFIRM_TIMEOUT: Duration = Duration::from_secs(30);
/// Poll interval of the fence confirmation.
const FENCE_CONFIRM_POLL: Duration = Duration::from_millis(50);
/// Grace period between the placement flip and the source cleanup
/// (straggler readers on the source re-route within it).
const CLEANUP_GRACE: Duration = Duration::from_millis(500);
/// Redirect re-resolution budget of one driver RPC.
const MAX_DRIVER_REDIRECTS: u32 = 8;

/// The outcome of one completed move/split/merge.
pub(crate) struct MoveOutcome {
    /// The moved/split-child/merged-left partition id.
    pub partition_id: OID,
    /// The target group.
    pub group_id: u64,
    /// The serving node of the target group at completion.
    pub node_id: Option<u64>,
    /// The serving node's client address, when known.
    pub client_addr: Option<String>,
    /// The placement version at the flip (the MetaGroup raft index).
    pub placement_version: u64,
    /// Rows re-homed (splits/merges; 0 for moves).
    pub rows_rehomed: u64,
}

/// The migration driver: partition moves, splits, and merges over the
/// cluster directory, the MetaGroup control log, and the migration
/// partition RPCs.
pub(crate) struct MigrationDriver {
    pub(crate) directory: Arc<ClusterDirectory>,
    pub(crate) channel: mudu_utils::channel::Channel,
    pub(crate) meta_group: Arc<dyn MetaGroupApi>,
    pub(crate) meta_mgr: Arc<dyn MetaMgr>,
}

impl MigrationDriver {
    /// Builds the driver for this node's cluster; `None` without cluster
    /// mode (callers report "not supported").
    pub(crate) fn for_server(
        server_instance_id: OID,
        meta_mgr: Arc<dyn MetaMgr>,
    ) -> RS<Option<Self>> {
        let Some(directory) =
            ::server::server::cluster_directory::cluster_directory_for(server_instance_id)
        else {
            return Ok(None);
        };
        let node_endpoint =
            catalog::contract::derive_oid::derive_node_endpoint(directory.self_node_id());
        let channel = mudu_utils::channel::Channel::new(mudu_utils::channel::channel_for(
            server_instance_id,
            node_endpoint,
        )?);
        let meta_group: Arc<dyn MetaGroupApi> =
            Arc::new(xcontract::meta_group::ChannelMetaGroup::with_channel(
                directory.self_node_id(),
                channel.clone(),
            ));
        Ok(Some(Self {
            directory,
            channel,
            meta_group,
            meta_mgr,
        }))
    }

    /// The driver's cluster directory (the scheduler reads placements).
    pub(crate) fn directory(&self) -> &Arc<ClusterDirectory> {
        &self.directory
    }

    /// The driver's catalog view.
    pub(crate) fn meta_mgr(&self) -> &Arc<dyn MetaMgr> {
        &self.meta_mgr
    }

    /// Drives one partition move: online copy, partition-local cutover
    /// quiescence, placement flip, source cleanup. Resumes an interrupted
    /// move of the same partition to the same target; rejects a
    /// conflicting one.
    pub(crate) async fn move_partition(&self, request: PartitionMoveRequest) -> RS<MoveOutcome> {
        let plan = self
            .validate_move(
                &request.rule,
                &request.partition,
                request.group_id,
                request.worker_index,
            )
            .await?;
        let outcome = self.drive_move(&plan).await;
        if let Err(err) = &outcome {
            warn!(
                rule = %plan.rule,
                partition = %plan.partition,
                "partition move failed; the migration fence stays until an abort or a retry: {err}"
            );
        }
        outcome
    }

    /// Aborts an in-flight partition move: the fence lifts and the source
    /// keeps serving (the target's staged copy is dropped).
    pub(crate) async fn abort_partition_move(&self, request: PartitionMoveAbortRef<'_>) -> RS<()> {
        let (rule, partition) = (request.rule, request.partition);
        let state = self.directory.migration_state(rule, partition);
        if !matches!(state, MigrationState::Migrating { .. }) {
            return Err(mudu_error!(
                ErrorCode::ObjectNotFound,
                format!("partition {rule}.{partition} is not migrating")
            ));
        }
        self.meta_group
            .apply(MetaOp::AbortMigration {
                rule: rule.to_string(),
                partition: partition.to_string(),
            })
            .await?;
        // Best-effort: drop the target's staged copy on every member.
        if let MigrationState::Migrating {
            target_group,
            target_worker_index,
            ..
        } = state
        {
            let partition_id = derive_partition_oid(rule, partition);
            self.drop_on_group_members((target_group, target_worker_index), partition_id)
                .await;
        }
        Ok(())
    }

    /// Places a never-placed partition (created at runtime, so without a
    /// static placement): a pure overlay assignment — Begin + Complete with
    /// no data copy (no node served it yet).
    pub(crate) async fn place_partition(
        &self,
        rule: String,
        partition: String,
        target_group: u64,
        target_worker_index: u32,
    ) -> RS<MoveOutcome> {
        let rule_desc = self.rule_by_name(&rule).await?;
        let def = partition_def(&rule_desc, &partition)?;
        let partition_id = derive_partition_oid(&rule, &partition);
        if def.partition_id != partition_id {
            return Err(mudu_error!(
                ErrorCode::InvalidArgument,
                format!(
                    "partition {rule}.{partition} has a random oid and cannot be placed; recreate the partition rule to enable migration"
                )
            ));
        }
        let Some(target) = self.directory.topology().group(target_group) else {
            return Err(mudu_error!(
                ErrorCode::ObjectNotFound,
                format!("no such cluster group {target_group}")
            ));
        };
        if target_worker_index >= target.worker_count {
            return Err(mudu_error!(
                ErrorCode::InvalidArgument,
                format!(
                    "group {} has {} workers, no worker index {}",
                    target_group, target.worker_count, target_worker_index
                )
            ));
        }
        if self.directory.placement(&rule, &partition).is_some() {
            return Err(mudu_error!(
                ErrorCode::InvalidArgument,
                format!("partition {rule}.{partition} already has a placement; move it instead")
            ));
        }
        if matches!(
            self.directory.migration_state(&rule, &partition),
            MigrationState::Migrating { .. }
        ) {
            return Err(mudu_error!(
                ErrorCode::ResourceBusy,
                format!("partition {rule}.{partition} is migrating")
            ));
        }
        self.meta_group
            .apply(MetaOp::BeginMigration {
                rule: rule.clone(),
                partition: partition.clone(),
                partition_id,
                target_group,
                target_worker_index,
                fence_reads: false,
            })
            .await?;
        // The partition has no data anywhere; the target group just
        // registers the ownership markers.
        let source_worker = self.group_worker_endpoint(target_group, target_worker_index)?;
        self.register_on_group_members(target_group, partition_id, source_worker)
            .await?;
        self.meta_group
            .apply(MetaOp::CompleteMigration {
                rule: rule.clone(),
                partition: partition.clone(),
            })
            .await?;
        let node = self.directory.serving_node(target_group);
        Ok(MoveOutcome {
            partition_id,
            group_id: target_group,
            node_id: node,
            client_addr: node
                .and_then(|node| self.directory.node_client_addr(node))
                .map(|(host, port)| format!("{host}:{port}")),
            placement_version: self.directory.placement_version(),
            rows_rehomed: 0,
        })
    }
}

/// Borrowed form of the abort request (avoids a DTO dependency here).
pub(crate) struct PartitionMoveAbortRef<'a> {
    /// Name of the partition rule.
    pub rule: &'a str,
    /// Name of the partition.
    pub partition: &'a str,
}

/// The validated move plan.
pub(crate) struct MovePlan {
    rule: String,
    partition: String,
    partition_id: OID,
    source: (u64, u32),
    target: (u64, u32),
}

/// The resolved identifiers one split/merge body needs.
pub(crate) struct SplitMergePlan {
    /// The rule's oid.
    pub(crate) rule_id: OID,
    /// Parent (split) / surviving-left (merge) partition id.
    pub(crate) primary_partition_id: OID,
    /// Child (split) / dropped-right (merge) partition id.
    pub(crate) secondary_partition_id: OID,
    /// The owner worker endpoint of both partitions.
    pub(crate) owner: OID,
}

impl MigrationDriver {
    /// Validates a move request and resolves source/target.
    async fn validate_move(
        &self,
        rule: &str,
        partition: &str,
        target_group: u64,
        target_worker_index: Option<u32>,
    ) -> RS<MovePlan> {
        let rule_desc = self.rule_by_name(rule).await?;
        let def = partition_def(&rule_desc, partition)?;
        let partition_id = derive_partition_oid(rule, partition);
        if def.partition_id != partition_id {
            return Err(mudu_error!(
                ErrorCode::InvalidArgument,
                format!(
                    "partition {rule}.{partition} has a random oid and cannot move; recreate the partition rule to enable migration"
                )
            ));
        }
        let Some(target) = self.directory.topology().group(target_group) else {
            return Err(mudu_error!(
                ErrorCode::ObjectNotFound,
                format!("no such cluster group {target_group}")
            ));
        };
        let target_worker_index = target_worker_index.unwrap_or(0);
        if target_worker_index >= target.worker_count {
            return Err(mudu_error!(
                ErrorCode::InvalidArgument,
                format!(
                    "group {} has {} workers, no worker index {}",
                    target_group, target.worker_count, target_worker_index
                )
            ));
        }
        let Some(source) = self.directory.placement(rule, partition) else {
            return Err(mudu_error!(
                ErrorCode::ObjectNotFound,
                format!("partition {rule}.{partition} has no cluster placement")
            ));
        };
        if source == (target_group, target_worker_index) {
            return Err(mudu_error!(
                ErrorCode::InvalidArgument,
                format!(
                    "partition {rule}.{partition} is already on group {target_group} worker {target_worker_index}"
                )
            ));
        }
        match self.directory.migration_state(rule, partition) {
            MigrationState::Migrating {
                target_group: g,
                target_worker_index: w,
                ..
            } if (g, w) == (target_group, target_worker_index) => {
                // Resume an interrupted move to the same target.
            }
            MigrationState::Migrating { .. } => {
                return Err(mudu_error!(
                    ErrorCode::ResourceBusy,
                    format!("partition {rule}.{partition} is already migrating; abort it first")
                ));
            }
            MigrationState::None => {}
        }
        Ok(MovePlan {
            rule: rule.to_string(),
            partition: partition.to_string(),
            partition_id,
            source,
            target: (target_group, target_worker_index),
        })
    }

    /// The move itself: online copy → cutover quiescence → final copy →
    /// placement flip → source cleanup.
    async fn drive_move(&self, plan: &MovePlan) -> RS<MoveOutcome> {
        let source_worker = self.group_worker_endpoint(plan.source.0, plan.source.1)?;

        // Begin: fence new writes on the source as every node applies.
        self.meta_group
            .apply(MetaOp::BeginMigration {
                rule: plan.rule.clone(),
                partition: plan.partition.clone(),
                partition_id: plan.partition_id,
                target_group: plan.target.0,
                target_worker_index: plan.target.1,
                // A move keeps the source readable during the window.
                fence_reads: false,
            })
            .await?;

        // Online copy: every member of the target group pulls the
        // partition's snapshot from the source owner (replicas hold the
        // base data the group's WAL stream never carries).
        self.adopt_on_group_members(plan.target, plan.partition_id, source_worker)
            .await?;

        // Cutover quiescence, fully partition-local on the source owner:
        // the fence rejects new writes (`Migrating`), in-doubt 2PC prepares
        // settle (`CommitTx`/`AbortTx` pass the fence by design), and the
        // in-flight drain observes every fenced write that started before
        // the fence went up (register-first at the RPC door). No global
        // epoch barrier on this path: a cluster-wide drain is hostage to
        // unrelated long transactions on any node.
        self.confirm_fence(source_worker, plan.partition_id).await?;

        // Final copy: after the fence the source partition is quiescent,
        // so this pass captures every write and is authoritative (every
        // member pulls the identical quiescent state).
        let adopted = self
            .adopt_on_group_members(plan.target, plan.partition_id, source_worker)
            .await?;

        // Flip the placement cluster-wide; the raft index of this record is
        // the placement version every node's overlay reports.
        self.meta_group
            .apply(MetaOp::CompleteMigration {
                rule: plan.rule.clone(),
                partition: plan.partition.clone(),
            })
            .await?;
        let placement_version = self.directory.placement_version();

        // Cleanup: grace for straggler readers, then drop the source copy
        // on every member of the source group.
        mudu_sys::sleep(CLEANUP_GRACE).await?;
        self.drop_on_group_members(plan.source, plan.partition_id)
            .await;
        let files_copied = match adopted {
            PartitionRpcResponse::PartitionAdopted(files) => files,
            _ => 0,
        };
        debug!(
            rule = %plan.rule,
            partition = %plan.partition,
            from = ?plan.source,
            to = ?plan.target,
            files_copied,
            placement_version,
            "partition move completed"
        );
        let node = self.directory.serving_node(plan.target.0);
        Ok(MoveOutcome {
            partition_id: plan.partition_id,
            group_id: plan.target.0,
            node_id: node,
            client_addr: node
                .and_then(|node| self.directory.node_client_addr(node))
                .map(|(host, port)| format!("{host}:{port}")),
            placement_version,
            rows_rehomed: 0,
        })
    }

    /// The partition rule by name (object-not-found when missing).
    pub(crate) async fn rule_by_name(&self, rule: &str) -> RS<PartitionRuleDesc> {
        self.meta_mgr
            .get_partition_rule_by_name(rule)
            .await?
            .ok_or_else(|| {
                mudu_error!(
                    ErrorCode::ObjectNotFound,
                    format!("partition rule {rule} not found")
                )
            })
    }

    /// v1 restriction: split/merge reject rules with a bound queue (queue
    /// shard state lives outside the relation rows the redistribution
    /// moves).
    pub(crate) async fn reject_queue_bindings(&self, rule_id: OID) -> RS<()> {
        for schema in self.meta_mgr.list_schemas().await? {
            if schema.kind() != SchemaClassKind::Queue {
                continue;
            }
            if let Some(binding) = self
                .meta_mgr
                .get_table_partition_binding(schema.id())
                .await?
                && binding.rule_id == rule_id
            {
                return Err(mudu_error!(
                    ErrorCode::NotImplemented,
                    format!(
                        "split/merge of a rule with a bound queue ({}) is not supported",
                        schema.class_name()
                    )
                ));
            }
        }
        Ok(())
    }

    /// v1 restriction: the split's key comparison projects stored keys by
    /// the partition-key prefix, so every bound table's partition key must
    /// be the leading columns of its primary key (in datum order).
    pub(crate) async fn validate_split_key_prefix(&self, rule_id: OID) -> RS<()> {
        for schema in self.meta_mgr.list_schemas().await? {
            let Some(binding) = self
                .meta_mgr
                .get_table_partition_binding(schema.id())
                .await?
            else {
                continue;
            };
            if binding.rule_id != rule_id {
                continue;
            }
            let desc = self.meta_mgr.get_table_by_id(schema.id()).await?;
            let mut key_datum: Vec<usize> = desc
                .key_indices()
                .iter()
                .map(|attr| desc.get_attr(*attr).datum_index())
                .collect();
            key_datum.sort_unstable();
            let mut route_datum: Vec<usize> = binding
                .ref_attr_indices
                .iter()
                .map(|attr| desc.get_attr(*attr).datum_index())
                .collect();
            route_datum.sort_unstable();
            if route_datum != key_datum[..route_datum.len()] {
                return Err(mudu_error!(
                    ErrorCode::NotImplemented,
                    format!(
                        "split requires the partition key to lead the primary key (table {})",
                        schema.class_name()
                    )
                ));
            }
        }
        Ok(())
    }

    /// `PartitionAdopt` against the worker of every member node of the
    /// target group (the group's replicas must hold the base data the WAL
    /// stream never carried). Returns the last member's adopted file count.
    pub(crate) async fn adopt_on_group_members(
        &self,
        target: (u64, u32),
        partition_id: OID,
        source_worker: OID,
    ) -> RS<PartitionRpcResponse> {
        let group = self
            .directory
            .topology()
            .group(target.0)
            .ok_or_else(|| {
                mudu_error!(
                    ErrorCode::ObjectNotFound,
                    format!("no such cluster group {}", target.0)
                )
            })?
            .clone();
        let mut adopted = PartitionRpcResponse::PartitionAdopted(0);
        for member in &group.members {
            let endpoint = derive_worker_oid(member.node_id, target.1 as usize);
            let response = self
                .ask_worker_rpc(
                    endpoint,
                    PartitionRpcRequest::PartitionAdopt {
                        partition_id,
                        source_worker,
                        register_only: false,
                    },
                )
                .await?;
            if let PartitionRpcResponse::Err(err) = &response {
                return Err(mudu_error!(ErrorCode::Internal, err.clone()));
            }
            adopted = response;
        }
        Ok(adopted)
    }

    /// `PartitionDrop` against the worker of every member node of the
    /// group (best-effort: a failed member keeps an inert fenced copy,
    /// cleaned up by a retried move).
    pub(crate) async fn drop_on_group_members(&self, source: (u64, u32), partition_id: OID) {
        let Some(group) = self.directory.topology().group(source.0).cloned() else {
            return;
        };
        for member in &group.members {
            let endpoint = derive_worker_oid(member.node_id, source.1 as usize);
            if let Err(err) = self
                .ask_worker_rpc(
                    endpoint,
                    PartitionRpcRequest::PartitionDrop { partition_id },
                )
                .await
            {
                warn!(
                    partition_id,
                    node_id = member.node_id,
                    "source partition cleanup failed: {err}"
                );
            }
        }
    }

    /// `PartitionAdopt{register_only}` on every member node of the group:
    /// the child partition's ownership markers land everywhere (the rows
    /// reach the replicas through the group's WAL stream).
    pub(crate) async fn register_on_group_members(
        &self,
        group_id: u64,
        partition_id: OID,
        source_worker: OID,
    ) -> RS<()> {
        let Some(group) = self.directory.topology().group(group_id).cloned() else {
            return Ok(());
        };
        let worker_index = worker_index_in_group(&self.directory, group_id, source_worker);
        for member in &group.members {
            let endpoint = derive_worker_oid(member.node_id, worker_index);
            self.ask_worker_rpc(
                endpoint,
                PartitionRpcRequest::PartitionAdopt {
                    partition_id,
                    source_worker,
                    register_only: true,
                },
            )
            .await?;
        }
        Ok(())
    }

    /// The endpoint id of `(group, worker_index)` on the group's serving
    /// node.
    pub(crate) fn group_worker_endpoint(&self, group_id: u64, worker_index: u32) -> RS<OID> {
        let node = self.directory.serving_node(group_id).ok_or_else(|| {
            mudu_error!(
                ErrorCode::InvalidState,
                format!("cluster group {group_id} has no serving node")
            )
        })?;
        Ok(derive_worker_oid(node, worker_index as usize))
    }

    /// Polls the source owner until its overlay fences the partition, no
    /// in-doubt 2PC prepare touches it, and no fenced write is in flight
    /// (the cutover quiescence point).
    pub(crate) async fn confirm_fence(&self, source_worker: OID, partition_id: OID) -> RS<()> {
        let deadline = *mudu_sys::time::instant_now() + FENCE_CONFIRM_TIMEOUT;
        loop {
            let response = self
                .ask_worker_rpc(
                    source_worker,
                    PartitionRpcRequest::MigrationFenceQuery { partition_id },
                )
                .await;
            if let Ok(PartitionRpcResponse::MigrationFence {
                fenced: true,
                prepared_pending: false,
                inflight: false,
            }) = response
            {
                return Ok(());
            }
            if *mudu_sys::time::instant_now() > deadline {
                return Err(mudu_error!(
                    ErrorCode::TimedOut,
                    format!("source owner did not fence partition {partition_id} in time")
                ));
            }
            mudu_sys::sleep(FENCE_CONFIRM_POLL).await?;
        }
    }

    /// Asks one worker endpoint with redirect re-resolution (group failover
    /// mid-move rotates to the new leader).
    pub(crate) async fn ask_worker_rpc(
        &self,
        target: OID,
        request: PartitionRpcRequest,
    ) -> RS<PartitionRpcResponse> {
        let mut target = target;
        let mut redirects = 0;
        loop {
            let response = self
                .channel
                .endpoint::<PartitionRpcRequest>(target)
                .ask(request.clone(), MIGRATION_RPC_TIMEOUT)
                .await;
            match response {
                Ok(PartitionRpcResponse::Redirect { node_id })
                    if redirects < MAX_DRIVER_REDIRECTS =>
                {
                    redirects += 1;
                    // Learn the group's serving node and re-resolve the
                    // worker endpoint through the directory.
                    if let Some(node) = node_id
                        && let Some(group) = self.directory.topology().group_of_node(node)
                    {
                        self.directory.note_serving_node(group.group_id, node);
                    }
                    let Some(node) = self.directory.node_of_worker(target) else {
                        return Err(mudu_error!(
                            ErrorCode::ObjectNotFound,
                            format!("worker {target} is not in the cluster topology")
                        ));
                    };
                    let Some(group) = self.directory.topology().group_of_node(node) else {
                        return Err(mudu_error!(
                            ErrorCode::ObjectNotFound,
                            format!("node {node} is not in the cluster topology")
                        ));
                    };
                    let serving = self.directory.serving_node(group.group_id).ok_or_else(|| {
                        mudu_error!(
                            ErrorCode::InvalidState,
                            format!("cluster group {} has no serving node", group.group_id)
                        )
                    })?;
                    let worker_index =
                        worker_index_of(&self.directory, target).ok_or_else(|| {
                            mudu_error!(
                                ErrorCode::ObjectNotFound,
                                format!("worker {target} is not derivable in the cluster topology")
                            )
                        })?;
                    target = derive_worker_oid(serving, worker_index as usize);
                }
                Ok(response) => return Ok(response),
                Err(err) => return Err(err),
            }
        }
    }
}

/// The named partition of the rule (object-not-found when missing).
pub(crate) fn partition_def<'a>(
    rule: &'a PartitionRuleDesc,
    partition: &str,
) -> RS<&'a RangePartitionDef> {
    rule.partitions
        .iter()
        .find(|def| def.name == partition)
        .ok_or_else(|| {
            mudu_error!(
                ErrorCode::ObjectNotFound,
                format!("partition {}.{partition} not found", rule.name)
            )
        })
}

/// The group of a worker endpoint (via its node).
pub(crate) fn parent_group_of(directory: &Arc<ClusterDirectory>, worker: OID) -> RS<u64> {
    let node = directory.node_of_worker(worker).ok_or_else(|| {
        mudu_error!(
            ErrorCode::ObjectNotFound,
            format!("worker {worker} is not in the cluster topology")
        )
    })?;
    Ok(directory
        .topology()
        .group_of_node(node)
        .ok_or_else(|| {
            mudu_error!(
                ErrorCode::ObjectNotFound,
                format!("node {node} is not in the cluster topology")
            )
        })?
        .group_id)
}

/// The worker index of a derived worker endpoint id within its node.
pub(crate) fn worker_index_of(directory: &Arc<ClusterDirectory>, worker: OID) -> Option<u32> {
    let node = directory.node_of_worker(worker)?;
    let group = directory.topology().group_of_node(node)?;
    (0..group.worker_count).find(|&index| derive_worker_oid(node, index as usize) == worker)
}

/// The worker index of a derived worker endpoint id within its group.
pub(crate) fn worker_index_in_group(
    directory: &Arc<ClusterDirectory>,
    group_id: u64,
    worker: OID,
) -> usize {
    let Some(node) = directory.node_of_worker(worker) else {
        return 0;
    };
    let Some(group) = directory.topology().group(group_id) else {
        return 0;
    };
    (0..group.worker_count as usize)
        .find(|&index| derive_worker_oid(node, index) == worker)
        .unwrap_or(0)
}
