//! The routing contract between the transaction layer (`xcontract`) and the
//! cluster directory implemented in `server`: resolves a statically placed
//! `(group, worker_index)` to the channel endpoint id of the worker on the
//! group's currently-serving node.
//!
//! Implemented by `server::server::cluster_directory::ClusterDirectory`;
//! consumed through `WorkerXContract`. Pure-contract crate placement keeps
//! the layering rule (xcontract must not depend on server).

use mudu::common::id::OID;
use mudu::common::result::RS;

/// The migration state of a partition in the routing overlay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MigrationState {
    /// No migration in progress.
    None,
    /// The partition is moving to another node group: the current owner
    /// fences new writes (the client retries) until the placement flips or
    /// the migration aborts.
    Migrating {
        /// Target node group.
        target_group: u64,
        /// Target worker index within the group's nodes.
        target_worker_index: u32,
        /// Whether the fence also covers reads (splits/merges re-home rows
        /// in place; moves keep the source readable during the window).
        fence_reads: bool,
    },
}

/// Cluster routing directory consumed by the worker contract.
pub trait ClusterRouter: Send + Sync {
    /// The fencing version of the topology this router resolves against.
    fn topology_version(&self) -> u64;

    /// The group this node belongs to.
    fn self_group_id(&self) -> u64;

    /// Current durable incarnation of a known data group.
    fn group_generation(&self, group_id: u64) -> RS<u64>;

    /// This node's cluster node id.
    fn self_node_id(&self) -> u64;

    /// The static placement of `(rule, partition)`: `(group_id,
    /// worker_index)`; `None` when the partition has no placement.
    fn placement(&self, rule: &str, partition: &str) -> Option<(u64, u32)>;

    /// Resolves `(group_id, worker_index)` to the endpoint id of that worker
    /// on the group's currently-serving node.
    fn resolve_group_worker(&self, group_id: u64, worker_index: u32) -> RS<OID>;

    /// Structs that `group_id` is served by `node_id` (learned from a
    /// redirect or an observed failure; the serving-node cache is the only
    /// dynamic state of the static topology).
    fn note_serving_node(&self, group_id: u64, node_id: u64);

    /// Structs that the believed serving node of `group_id` failed; the
    /// serving guess rotates to another member of the group (definitive
    /// information arrives later through `note_serving_node`).
    fn note_serving_failure(&self, group_id: u64, node_id: u64);

    /// The node currently believed to serve `group_id`, when known.
    fn serving_node(&self, group_id: u64) -> Option<u64>;

    /// The migration state of `(rule, partition)` in the dynamic placement
    /// overlay (phase 3): the write path fences `Migrating` partitions.
    fn migration_state(&self, rule: &str, partition: &str) -> MigrationState;

    /// Monotonic version of the dynamic placement overlay (the raft index
    /// of the last placement-affecting meta op applied on this node);
    /// 0 when no dynamic placement was ever applied.
    fn placement_version(&self) -> u64;
}
