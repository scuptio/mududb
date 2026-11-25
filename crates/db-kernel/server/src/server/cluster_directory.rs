//! The static cluster directory: the node-local view of the cluster
//! topology plus the per-group serving-node cache and the dynamic placement
//! overlay.
//!
//! Worker ids are derived from `(node_id, worker_index)`
//! (`catalog::contract::derive_oid::derive_worker_oid`), so the directory
//! answers `worker_id → node` and `(group, worker_index) → worker_id` by
//! pure computation over the static topology; failover learning happens
//! through [`ClusterDirectory::note_serving_node`] (fed by redirect
//! responses and observed failures). Partition moves flip placements
//! through the overlay: the control plane's catalog stream replicates the
//! migration records and every node applies them to its overlay (persisted
//! locally so a restart keeps the fencing state).

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};

use ::catalog::contract::cluster_router::{ClusterRouter, MigrationState};
use ::catalog::contract::cluster_topology::{ClusterGroup, ClusterTopology};
use ::catalog::contract::control_plane::CONTROL_GROUP_ID;
use ::catalog::contract::derive_oid::derive_worker_oid;
use ::catalog::contract::meta_op::MetaOp;
use ::mudu_raft::stream::{GroupId, GroupIdobject};
use mudu::channel::ServerInstanceId;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_sys::sync::SMutex;
use serde::{Deserialize, Serialize};

/// Per-server-instance registry of cluster directories, so the management
/// HTTP layer (another crate) reads the live routing view of the kernel.
pub fn register_cluster_directory(
    server_instance_id: ServerInstanceId,
    directory: Arc<ClusterDirectory>,
) -> RS<()> {
    cluster_directory_registry()
        .lock()
        .map_err(|_| mudu_error!(ErrorCode::Internal, "cluster directory registry poisoned"))?
        .insert(server_instance_id, directory);
    Ok(())
}

/// Removes the cluster directory registration of a server instance.
pub fn unregister_cluster_directory(server_instance_id: ServerInstanceId) -> RS<()> {
    cluster_directory_registry()
        .lock()
        .map_err(|_| mudu_error!(ErrorCode::Internal, "cluster directory registry poisoned"))?
        .remove(&server_instance_id);
    Ok(())
}

/// The cluster directory registered for a server instance, if any.
pub fn cluster_directory_for(
    server_instance_id: ServerInstanceId,
) -> Option<Arc<ClusterDirectory>> {
    cluster_directory_registry()
        .lock()
        .ok()?
        .get(&server_instance_id)
        .cloned()
}

/// Registers the node's epoch oracle (the migration driver waits on its
/// barrier); `None` removes the registration.
pub fn register_cluster_epoch_oracle(
    server_instance_id: ServerInstanceId,
    oracle: Arc<dyn ::catalog::contract::epoch_oracle::EpochOracle>,
) -> RS<()> {
    epoch_oracle_registry()
        .lock()
        .map_err(|_| mudu_error!(ErrorCode::Internal, "epoch oracle registry poisoned"))?
        .insert(server_instance_id, oracle);
    Ok(())
}

/// Removes the epoch oracle registration of a server instance.
pub fn unregister_cluster_epoch_oracle(server_instance_id: ServerInstanceId) -> RS<()> {
    epoch_oracle_registry()
        .lock()
        .map_err(|_| mudu_error!(ErrorCode::Internal, "epoch oracle registry poisoned"))?
        .remove(&server_instance_id);
    Ok(())
}

/// The epoch oracle registered for a server instance, if any.
pub fn cluster_epoch_oracle_for(
    server_instance_id: ServerInstanceId,
) -> Option<Arc<dyn ::catalog::contract::epoch_oracle::EpochOracle>> {
    epoch_oracle_registry()
        .lock()
        .ok()?
        .get(&server_instance_id)
        .cloned()
}

/// Registers the node's meta-leadership probe (the rebalance scheduler
/// acts only on the control-plane leader).
pub fn register_meta_leadership(
    server_instance_id: ServerInstanceId,
    probe: Arc<dyn Fn() -> bool + Send + Sync>,
) -> RS<()> {
    meta_leadership_registry()
        .lock()
        .map_err(|_| mudu_error!(ErrorCode::Internal, "meta leadership registry poisoned"))?
        .insert(server_instance_id, probe);
    Ok(())
}

/// Removes the meta-leadership registration of a server instance.
pub fn unregister_meta_leadership(server_instance_id: ServerInstanceId) -> RS<()> {
    meta_leadership_registry()
        .lock()
        .map_err(|_| mudu_error!(ErrorCode::Internal, "meta leadership registry poisoned"))?
        .remove(&server_instance_id);
    Ok(())
}

/// Whether this node is the control-plane leader, when registered.
pub fn is_meta_leader(server_instance_id: ServerInstanceId) -> Option<bool> {
    meta_leadership_registry()
        .lock()
        .ok()?
        .get(&server_instance_id)
        .map(|probe| probe())
}

/// The overlay effect of one placement-affecting meta op: the
/// `(rule, partition)` key plus the new override (`None` clears).
type PlacementEffect = ((String, String), Option<PlacementOverride>);

/// The meta-leadership probe of a node (the rebalance scheduler reads it).
type MetaLeadershipProbe = Arc<dyn Fn() -> bool + Send + Sync>;

fn meta_leadership_registry() -> &'static SMutex<HashMap<ServerInstanceId, MetaLeadershipProbe>> {
    static REGISTRY: OnceLock<SMutex<HashMap<ServerInstanceId, MetaLeadershipProbe>>> =
        OnceLock::new();
    REGISTRY.get_or_init(|| SMutex::new(HashMap::new()))
}

fn epoch_oracle_registry() -> &'static SMutex<
    HashMap<ServerInstanceId, Arc<dyn ::catalog::contract::epoch_oracle::EpochOracle>>,
> {
    static REGISTRY: OnceLock<
        SMutex<HashMap<ServerInstanceId, Arc<dyn ::catalog::contract::epoch_oracle::EpochOracle>>>,
    > = OnceLock::new();
    REGISTRY.get_or_init(|| SMutex::new(HashMap::new()))
}

fn cluster_directory_registry() -> &'static SMutex<HashMap<ServerInstanceId, Arc<ClusterDirectory>>>
{
    static REGISTRY: OnceLock<SMutex<HashMap<ServerInstanceId, Arc<ClusterDirectory>>>> =
        OnceLock::new();
    REGISTRY.get_or_init(|| SMutex::new(HashMap::new()))
}

/// One dynamic placement overlay entry, applied from the catalog stream.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum PlacementOverride {
    /// The partition is migrating to the target; the current owner fences
    /// new writes until the migration completes or aborts.
    Migrating {
        /// Target node group.
        target_group: u64,
        /// Target worker index within the group's nodes.
        target_worker_index: u32,
        /// Whether the fence also covers reads (splits/merges re-home rows
        /// in place; moves keep the source readable during the window).
        fence_reads: bool,
    },
    /// The partition moved; this is the effective placement.
    Moved {
        /// Owning node group.
        group_id: u64,
        /// Worker index within the group's nodes.
        worker_index: u32,
    },
}

mod overlay;
mod recovery;
use overlay::{load_overlay, save_overlay, OverlayCursor};

/// The cluster directory of one node.
pub struct ClusterDirectory {
    topology: ClusterTopology,
    self_node_id: u64,
    /// group_id -> serving node id; starts at the group's first member.
    serving: SMutex<HashMap<u64, u64>>,
    /// worker_id -> node_id, precomputed over the whole static topology.
    worker_node: HashMap<OID, u64>,
    /// `(rule, partition)` -> (override, control-stream LSN of the op that set it).
    overrides: SMutex<HashMap<(String, String), (PlacementOverride, u64)>>,
    /// Highest control-stream LSN of a placement operation applied locally.
    placement_version: AtomicU64,
    overlay_owner: tokio::sync::Mutex<Option<OverlayCursor>>,
    overlay_failed: std::sync::atomic::AtomicBool,
    /// Directory of the persisted overlay image (`data_dir`); `None` keeps
    /// the overlay memory-only (tests).
    overlay_dir: Option<std::path::PathBuf>,
}

impl ClusterDirectory {
    /// Builds the directory; fails when `self_node_id` belongs to no group.
    pub fn new(topology: ClusterTopology, self_node_id: u64) -> RS<Arc<Self>> {
        Self::new_with_overlay(topology, self_node_id, None)
    }

    /// Builds the directory and loads the persisted placement overlay from
    /// `overlay_dir` (`Some` in production: the node's data directory).
    pub fn new_with_overlay(
        topology: ClusterTopology,
        self_node_id: u64,
        overlay_dir: Option<std::path::PathBuf>,
    ) -> RS<Arc<Self>> {
        let mut worker_node = HashMap::new();
        for group in &topology.group {
            for member in &group.members {
                for worker_index in 0..group.worker_count {
                    worker_node.insert(
                        derive_worker_oid(member.node_id, worker_index as usize),
                        member.node_id,
                    );
                }
                // The node-level service endpoint (control-plane raft handler and
                // future node services) resolves the same way.
                worker_node.insert(
                    catalog::contract::derive_oid::derive_node_endpoint(member.node_id),
                    member.node_id,
                );
            }
        }
        // Dedicated control nodes also register a node-level endpoint so the
        // control raft handler and the stream RPCs resolve them even though
        // they host no workers.
        if let Some(control) = &topology.control {
            for node in &control.nodes {
                worker_node.insert(
                    catalog::contract::derive_oid::derive_node_endpoint(node.node_id),
                    node.node_id,
                );
            }
        }
        let (overrides, cursor) = match &overlay_dir {
            Some(dir) => load_overlay(dir, topology.control_cluster_id())?,
            None => (HashMap::new(), None),
        };
        let directory = Self {
            topology,
            self_node_id,
            serving: SMutex::new(HashMap::new()),
            worker_node,
            overrides: SMutex::new(overrides),
            placement_version: AtomicU64::new(cursor.as_ref().map_or(0, |c| c.version)),
            overlay_owner: tokio::sync::Mutex::new(cursor),
            overlay_failed: std::sync::atomic::AtomicBool::new(false),
            overlay_dir,
        };
        // The node must belong to a data group or the control plane.
        if directory.topology.role_of(self_node_id).is_none() {
            return Err(mudu_error!(
                ErrorCode::InvalidState,
                format!(
                    "cluster node {} belongs to no group and no control plane",
                    self_node_id
                )
            ));
        }
        Ok(Arc::new(directory))
    }

    /// The static topology.
    pub fn topology(&self) -> &ClusterTopology {
        &self.topology
    }

    /// This node's cluster node id.
    pub fn self_node_id(&self) -> u64 {
        self.self_node_id
    }

    /// The group this node belongs to.
    pub fn self_group(&self) -> RS<&ClusterGroup> {
        self.topology
            .group_of_node(self.self_node_id)
            .ok_or_else(|| {
                mudu_error!(
                    ErrorCode::InvalidState,
                    format!("cluster node {} belongs to no group", self.self_node_id)
                )
            })
    }

    /// The node hosting `worker_id`, when the id belongs to the topology.
    pub fn node_of_worker(&self, worker_id: OID) -> Option<u64> {
        self.worker_node.get(&worker_id).copied()
    }

    /// The data-plane channel address (`host:channel_port`) of a node.
    pub fn node_channel_addr(&self, node_id: u64) -> Option<(String, u16)> {
        self.topology.node_channel_addr(node_id)
    }

    /// The client (pgwire) address of a node; the management layer reports
    /// it for direct client routing.
    pub fn node_client_addr(&self, node_id: u64) -> Option<(String, u16)> {
        self.topology.node_client_addr(node_id)
    }

    /// Applies one placement-affecting meta op to the overlay (persisted
    /// first, then in memory). The control-stream LSN idempotently orders the
    /// application; returns `true` when the op was placement-affecting.
    pub async fn apply_placement_op(&self, op: &MetaOp, version: u64) -> RS<bool> {
        if !matches!(
            op,
            MetaOp::BeginMigration { .. }
                | MetaOp::CompleteMigration { .. }
                | MetaOp::AbortMigration { .. }
                | MetaOp::SplitPartition { .. }
        ) {
            return Ok(false);
        }
        let mut cursor = self.overlay_owner.lock().await;
        if self.overlay_failed.load(Ordering::Acquire) {
            return Err(mudu_error!(
                ErrorCode::InvalidState,
                "overlay publication interrupted; reopen required"
            ));
        }
        let payload = rmp_serde::to_vec(op)
            .map_err(|e| mudu_error!(ErrorCode::Encode, "encode placement operation", e))?;
        if let Some(previous) = cursor.as_ref() {
            if version < previous.version {
                return Ok(true);
            }
            if version == previous.version {
                return if payload == previous.payload {
                    Ok(true)
                } else {
                    Err(mudu_error!(
                        ErrorCode::InvalidState,
                        "conflicting placement operation at the same version"
                    ))
                };
            }
        }
        let effect = self.placement_effect(op)?;
        let mut entries = self.overrides.lock()?.clone();
        if let Some((key, value)) = effect {
            match value {
                Some(value) => {
                    entries.insert(key, (value, version));
                }
                None => {
                    entries.remove(&key);
                }
            }
        }
        let next = OverlayCursor { version, payload };
        // Cancellation or an ambiguous I/O result must not allow another write
        // to race the unfinished publication. Reopening revalidates durable state.
        self.overlay_failed.store(true, Ordering::Release);
        if let Some(dir) = &self.overlay_dir {
            save_overlay(dir, self.topology.control_cluster_id(), &entries, &next).await?;
        }
        *self.overrides.lock()? = entries;
        self.placement_version.store(version, Ordering::SeqCst);
        *cursor = Some(next);
        self.overlay_failed.store(false, Ordering::Release);
        Ok(true)
    }

    /// Maps a meta op to its overlay effect: `None` for
    /// non-placement-affecting ops; otherwise the `(rule, partition)` key
    /// and the new override (`None` clears the entry).
    fn placement_effect(&self, op: &MetaOp) -> RS<Option<PlacementEffect>> {
        let effect = match op {
            MetaOp::BeginMigration {
                rule,
                partition,
                target_group,
                target_worker_index,
                fence_reads,
                ..
            } => (
                (rule.clone(), partition.clone()),
                Some(PlacementOverride::Migrating {
                    target_group: *target_group,
                    target_worker_index: *target_worker_index,
                    fence_reads: *fence_reads,
                }),
            ),
            MetaOp::CompleteMigration { rule, partition } => {
                let key = (rule.clone(), partition.clone());
                let current = self
                    .overrides
                    .lock()
                    .map_err(|_| {
                        mudu_error!(ErrorCode::Internal, "placement overlay lock poisoned")
                    })?
                    .get(&key)
                    .cloned();
                let Some((
                    PlacementOverride::Migrating {
                        target_group,
                        target_worker_index,
                        ..
                    },
                    _,
                )) = current
                else {
                    // Complete without a begun migration: ignore (a replay
                    // after the flip must not clear the Moved entry).
                    return Ok(None);
                };
                (
                    key,
                    Some(PlacementOverride::Moved {
                        group_id: target_group,
                        worker_index: target_worker_index,
                    }),
                )
            }
            MetaOp::AbortMigration { rule, partition } => ((rule.clone(), partition.clone()), None),
            MetaOp::SplitPartition {
                rule,
                partition,
                new_partition,
                ..
            } => {
                // The child inherits the parent's effective placement and
                // starts fenced (reads included) until the redistribution
                // completes and the driver completes both fences.
                let parent_effective = ClusterRouter::placement(self, rule, partition);
                let Some((group_id, worker_index)) = parent_effective else {
                    return Err(mudu_error!(
                        ErrorCode::ObjectNotFound,
                        format!("split of {rule}.{partition}: the parent has no cluster placement")
                    ));
                };
                (
                    (rule.clone(), new_partition.clone()),
                    Some(PlacementOverride::Migrating {
                        target_group: group_id,
                        target_worker_index: worker_index,
                        fence_reads: true,
                    }),
                )
            }
            MetaOp::MergePartition { .. } => return Ok(None),
            _ => return Ok(None),
        };
        Ok(Some(effect))
    }

    /// Whether any partition is migrating (the rebalance scheduler stays
    /// out while a move is in flight).
    pub fn any_migrating(&self) -> bool {
        if self.overlay_failed.load(Ordering::Acquire) {
            return true;
        }
        self.overrides
            .lock()
            .map(|overrides| {
                overrides
                    .values()
                    .any(|(ov, _)| matches!(ov, PlacementOverride::Migrating { .. }))
            })
            .unwrap_or(false)
    }
}

impl ClusterRouter for ClusterDirectory {
    fn topology_version(&self) -> u64 {
        self.topology.version
    }

    fn self_group_id(&self) -> u64 {
        self.self_group().map(|group| group.group_id).unwrap_or(0)
    }

    fn group_generation(&self, group_id: u64) -> RS<u64> {
        self.topology.group(group_id).map(|_| 1).ok_or_else(|| {
            mudu_error!(
                ErrorCode::ObjectNotFound,
                "unknown coordinator participant group"
            )
        })
    }

    fn self_node_id(&self) -> u64 {
        self.self_node_id
    }

    fn placement(&self, rule: &str, partition: &str) -> Option<(u64, u32)> {
        // The dynamic overlay wins over the static placement (a completed
        // migration moved the partition); a migrating partition still
        // resolves to its source — the write path fences it separately.
        if let Ok(overrides) = self.overrides.lock() {
            if let Some((
                PlacementOverride::Moved {
                    group_id,
                    worker_index,
                },
                _,
            )) = overrides.get(&(rule.to_string(), partition.to_string()))
            {
                return Some((*group_id, *worker_index));
            }
        }
        self.topology
            .placement(rule, partition)
            .map(|placement| (placement.group_id, placement.worker_index))
    }

    fn resolve_group_worker(&self, group_id: u64, worker_index: u32) -> RS<OID> {
        if self.overlay_failed.load(Ordering::Acquire) {
            return Err(mudu_error!(
                ErrorCode::InvalidState,
                "placement publication is incomplete"
            ));
        }
        let group = self.topology.group(group_id).ok_or_else(|| {
            mudu_error!(
                ErrorCode::ObjectNotFound,
                format!("no such cluster group {}", group_id)
            )
        })?;
        if worker_index >= group.worker_count {
            return Err(mudu_error!(
                ErrorCode::ObjectNotFound,
                format!(
                    "group {} has {} workers, no worker index {}",
                    group_id, group.worker_count, worker_index
                )
            ));
        }
        let node_id = self.serving_node(group_id).ok_or_else(|| {
            mudu_error!(
                ErrorCode::InvalidState,
                format!("cluster group {} has no serving node", group_id)
            )
        })?;
        Ok(derive_worker_oid(node_id, worker_index as usize))
    }

    fn note_serving_node(&self, group_id: u64, node_id: u64) {
        let valid = self
            .topology
            .group(group_id)
            .is_some_and(|group| group.member(node_id).is_some());
        if !valid {
            return;
        }
        if let Ok(mut serving) = self.serving.lock() {
            serving.insert(group_id, node_id);
        }
    }

    fn note_serving_failure(&self, group_id: u64, node_id: u64) {
        let Some(group) = self.topology.group(group_id) else {
            return;
        };
        let Ok(mut serving) = self.serving.lock() else {
            return;
        };
        let believed = serving
            .get(&group_id)
            .copied()
            .or_else(|| group.members.first().map(|member| member.node_id));
        if believed != Some(node_id) {
            return;
        }
        // Rotate the guess to the next member of the group.
        let next = group
            .members
            .iter()
            .position(|member| member.node_id == node_id)
            .map(|index| group.members[(index + 1) % group.members.len()].node_id);
        if let Some(next) = next {
            serving.insert(group_id, next);
        }
    }

    fn serving_node(&self, group_id: u64) -> Option<u64> {
        let cached = self.serving.lock().ok()?.get(&group_id).copied();
        cached.or_else(|| {
            // Bootstrap default: the first member is the configured primary.
            self.topology
                .group(group_id)
                .and_then(|group| group.members.first().map(|member| member.node_id))
        })
    }

    fn migration_state(&self, rule: &str, partition: &str) -> MigrationState {
        if self.overlay_failed.load(Ordering::Acquire) {
            // Fence all routed reads/writes while publication is unresolved.
            return MigrationState::Migrating {
                target_group: self.self_group_id(),
                target_worker_index: 0,
                fence_reads: true,
            };
        }
        let Ok(overrides) = self.overrides.lock() else {
            return MigrationState::None;
        };
        match overrides.get(&(rule.to_string(), partition.to_string())) {
            Some((
                PlacementOverride::Migrating {
                    target_group,
                    target_worker_index,
                    fence_reads,
                },
                _,
            )) => MigrationState::Migrating {
                target_group: *target_group,
                target_worker_index: *target_worker_index,
                fence_reads: *fence_reads,
            },
            _ => MigrationState::None,
        }
    }

    fn placement_version(&self) -> u64 {
        self.placement_version.load(Ordering::SeqCst)
    }
}

/// The control group's stream identity: the shared replication group in the
/// Combined deployment, the dedicated control group in scale-out.
pub(crate) fn control_group_identity(topology: &ClusterTopology) -> GroupIdobject {
    GroupIdobject {
        cluster_id: topology.control_cluster_id(),
        group_id: GroupId(CONTROL_GROUP_ID),
        generation: 1,
    }
}

/// A node's data-group stream identity. In the Combined deployment the data
/// streams ride the control group (S-02: one shared replication group, so this
/// equals [`control_group_identity`]); in scale-out they use the node's
/// topology group.
pub(crate) fn data_group_identity(topology: &ClusterTopology, node_id: u64) -> GroupIdobject {
    let group_id = match &topology.control {
        None => CONTROL_GROUP_ID,
        Some(_) => topology
            .group_of_node(node_id)
            .map(|group| group.group_id)
            .unwrap_or(0),
    };
    GroupIdobject {
        cluster_id: topology.control_cluster_id(),
        group_id: GroupId(group_id),
        generation: 1,
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use ::catalog::contract::cluster_topology::ClusterGroupMember;

    fn member(node_id: u64) -> ClusterGroupMember {
        ClusterGroupMember {
            node_id,
            host: format!("10.0.0.{}", node_id),
            channel_port: 29527,
            client_port: 5432,
        }
    }

    fn cnode(node_id: u64) -> ::catalog::contract::cluster_topology::ControlPlaneNode {
        ::catalog::contract::cluster_topology::ControlPlaneNode {
            node_id,
            host: format!("10.0.0.{}", node_id),
            channel_port: 29528,
            client_port: 5432,
        }
    }

    pub(super) fn topology() -> ClusterTopology {
        ClusterTopology {
            version: 7,
            group: vec![
                ClusterGroup {
                    group_id: 1,
                    members: vec![member(1), member(2)],
                    worker_count: 2,
                },
                ClusterGroup {
                    group_id: 2,
                    members: vec![member(3)],
                    worker_count: 1,
                },
            ],
            placements: vec![],
            control: None,
            cluster_id: 0,
        }
    }

    #[test]
    fn directory_resolves_workers_deterministically() {
        let directory = ClusterDirectory::new(topology(), 1).unwrap();
        let w = directory.resolve_group_worker(2, 0).unwrap();
        assert_eq!(w, derive_worker_oid(3, 0));
        assert_eq!(directory.node_of_worker(w), Some(3));
        // Bootstrap serving node is the first member.
        assert_eq!(directory.serving_node(1), Some(1));
        assert_eq!(
            directory.resolve_group_worker(1, 1).unwrap(),
            derive_worker_oid(1, 1)
        );
        // Out-of-range worker index and unknown group fail.
        assert!(directory.resolve_group_worker(1, 2).is_err());
        assert!(directory.resolve_group_worker(9, 0).is_err());
    }

    #[test]
    fn combined_data_identity_equals_the_shared_control_group() {
        // Combined deployment: every node's data stream rides the control group.
        let combined = topology();
        assert!(combined.control.is_none());
        let control = control_group_identity(&combined);
        for node_id in [1u64, 2, 3] {
            assert_eq!(data_group_identity(&combined, node_id), control);
        }
        assert_eq!(control.group_id, GroupId(1));

        // Scale-out: the control group keeps its id, data nodes keep their own.
        let mut scale_out = topology();
        scale_out.control = Some(
            ::catalog::contract::cluster_topology::ControlPlaneTopology {
                nodes: vec![cnode(10)],
            },
        );
        assert_eq!(control_group_identity(&scale_out).group_id, GroupId(1));
        assert_eq!(data_group_identity(&scale_out, 3).group_id, GroupId(2));
        assert_eq!(data_group_identity(&scale_out, 1).group_id, GroupId(1));
    }

    #[test]
    fn serving_node_learning_validates_membership() {
        let directory = ClusterDirectory::new(topology(), 1).unwrap();
        directory.note_serving_node(1, 2);
        assert_eq!(directory.serving_node(1), Some(2));
        // A node outside the group is ignored.
        directory.note_serving_node(1, 3);
        assert_eq!(directory.serving_node(1), Some(2));
        assert_eq!(
            directory.resolve_group_worker(1, 0).unwrap(),
            derive_worker_oid(2, 0)
        );
    }

    #[test]
    fn self_group_required() {
        assert!(ClusterDirectory::new(topology(), 99).is_err());
        let directory = ClusterDirectory::new(topology(), 2).unwrap();
        assert_eq!(directory.self_group().unwrap().group_id, 1);
        assert_eq!(
            directory.node_channel_addr(3),
            Some(("10.0.0.3".to_string(), 29527))
        );
    }
}

#[cfg(test)]
mod overlay_tests;

#[cfg(test)]
mod recovery_tests;
