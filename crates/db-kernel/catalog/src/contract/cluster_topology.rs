//! Static cluster topology: the bootstrap form of the cluster directory
//! (the raft-backed unified control plane replaces this static form).
//!
//! The topology answers two routing questions without any RPC:
//! `partition (rule, name) → (group_id, worker_index)` and
//! `group_id → member nodes`. `version` is the fencing token: the operator
//! bumps it on every topology change, requests carry the version the
//! coordinator routed with, and owners reject mismatches with
//! `StaleRoute{current_version}`.

use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use serde::{Deserialize, Serialize};

/// One member node of a node group.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClusterGroupMember {
    /// Node id (same id space as raft/stream `node_id`).
    pub node_id: u64,
    /// Host name or IP of the node.
    pub host: String,
    /// Data-plane channel port of the node (the cluster listener).
    pub channel_port: u16,
    /// Client (pgwire) port of the node, used to build leader hints.
    pub client_port: u16,
}

/// A node group: the replication/failover unit hosting a set of shards
/// (partitions). All members run the same worker set; the group replicates
/// as one unit through the existing raft/stream machinery.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClusterGroup {
    /// Id of the group.
    pub group_id: u64,
    /// Member nodes; the first member is the configured bootstrap primary.
    pub members: Vec<ClusterGroupMember>,
    /// Workers per member node (identical across the group).
    pub worker_count: u32,
}

/// Static placement of one named partition of a partition rule.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct StaticPlacement {
    /// Name of the partition rule.
    pub rule: String,
    /// Name of the partition within the rule.
    pub partition: String,
    /// Owning node group.
    pub group_id: u64,
    /// Worker index within the group's nodes.
    pub worker_index: u32,
}

/// The static cluster topology of a multi-group cluster.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClusterTopology {
    /// Fencing token, bumped on every topology change.
    pub version: u64,
    /// The node groups of the cluster.
    pub group: Vec<ClusterGroup>,
    /// Static partition placements.
    pub placements: Vec<StaticPlacement>,
    /// Scale-out control-plane membership. `None` (the default) selects the
    /// Combined deployment: every node hosts both roles and votes in the
    /// single control replication group. `Some` fixes the control-plane
    /// voter set; the remaining nodes subscribe to the control replica set
    /// through a control client (CPC), not a Raft learner.
    #[serde(default)]
    pub control: Option<ControlPlaneTopology>,
    /// Stable cluster namespace of the control replication group. Zero (the
    /// default) derives it from the member set — sufficient while topologies
    /// are static; a deployment planning membership changes must pin an
    /// explicit id so the control group's identity survives them.
    #[serde(default)]
    pub cluster_id: u64,
}

/// One control-plane node of a scale-out deployment. Control nodes are
/// declared independently of data groups: they belong to the control
/// replication group only and never host business partitions.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ControlPlaneNode {
    /// Node id (same id space as raft/stream `node_id`).
    pub node_id: u64,
    /// Host name or IP of the control node.
    pub host: String,
    /// Control-plane channel port (the cluster listener).
    pub channel_port: u16,
    /// Client port of the control node (leader-hint / management routing).
    pub client_port: u16,
}

/// Fixed control-plane membership of a scale-out deployment. The control group
/// keeps one Raft replication group; the control nodes are declared here,
/// independent of (and disjoint from) the data groups. Data-plane nodes never
/// become control voters.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ControlPlaneTopology {
    /// The control-plane nodes (typically 3 or 5).
    pub nodes: Vec<ControlPlaneNode>,
}

/// The role a node plays, derived from the topology (A-01 of the control
/// plane design): a single instance owns one node id and one group
/// membership; roles never spawn additional replication groups.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeRole {
    /// The node hosts both the control plane (as a voter) and the data
    /// plane on one shared control replication group.
    Combined,
    /// The node is a fixed control-plane voter of a scale-out deployment.
    ControlPlane,
    /// The node serves the data plane only; it subscribes to the control
    /// replica set through a control client (CPC) and never votes.
    DataPlane,
}

impl ClusterGroup {
    /// Finds a member by node id.
    pub fn member(&self, node_id: u64) -> Option<&ClusterGroupMember> {
        self.members.iter().find(|member| member.node_id == node_id)
    }
}

impl ClusterTopology {
    /// Validates the topology: unique group ids, unique member node ids
    /// across the whole cluster, non-empty groups, and placements referencing
    /// existing groups with in-range worker indexes.
    pub fn validate(&self) -> RS<()> {
        let mut group_ids = std::collections::HashSet::new();
        let mut node_ids = std::collections::HashSet::new();
        for group in &self.group {
            Self::validate_group(group, &mut group_ids, &mut node_ids)?;
        }
        self.validate_placements()?;
        self.validate_control_nodes()?;
        Ok(())
    }

    /// Validates one data group: unique id, non-empty members, a positive
    /// worker count and cluster-unique member node ids.
    fn validate_group(
        group: &ClusterGroup,
        group_ids: &mut std::collections::HashSet<u64>,
        node_ids: &mut std::collections::HashSet<u64>,
    ) -> RS<()> {
        if !group_ids.insert(group.group_id) {
            return Err(mudu_error!(
                ErrorCode::InvalidArgument,
                format!("duplicate cluster group id {}", group.group_id)
            ));
        }
        if group.members.is_empty() {
            return Err(mudu_error!(
                ErrorCode::InvalidArgument,
                format!("cluster group {} has no members", group.group_id)
            ));
        }
        if group.worker_count == 0 {
            return Err(mudu_error!(
                ErrorCode::InvalidArgument,
                format!("cluster group {} has zero workers", group.group_id)
            ));
        }
        for member in &group.members {
            if !node_ids.insert(member.node_id) {
                return Err(mudu_error!(
                    ErrorCode::InvalidArgument,
                    format!(
                        "node id {} appears in more than one cluster group",
                        member.node_id
                    )
                ));
            }
        }
        Ok(())
    }

    /// Validates every placement: a known group, an in-range worker index and
    /// a unique `(rule, partition)` key.
    fn validate_placements(&self) -> RS<()> {
        let mut placement_keys = std::collections::HashSet::new();
        for placement in &self.placements {
            let group = self.group(placement.group_id).ok_or_else(|| {
                mudu_error!(
                    ErrorCode::InvalidArgument,
                    format!(
                        "placement of {}.{} references unknown group {}",
                        placement.rule, placement.partition, placement.group_id
                    )
                )
            })?;
            if placement.worker_index >= group.worker_count {
                return Err(mudu_error!(
                    ErrorCode::InvalidArgument,
                    format!(
                        "placement of {}.{} references worker {} but group {} has {} workers",
                        placement.rule,
                        placement.partition,
                        placement.worker_index,
                        group.group_id,
                        group.worker_count
                    )
                ));
            }
            if !placement_keys.insert((placement.rule.as_str(), placement.partition.as_str())) {
                return Err(mudu_error!(
                    ErrorCode::InvalidArgument,
                    format!(
                        "duplicate placement of partition {}.{}",
                        placement.rule, placement.partition
                    )
                ));
            }
        }
        Ok(())
    }

    /// Validates the control-plane node set: non-empty, unique, and disjoint
    /// from the data groups.
    fn validate_control_nodes(&self) -> RS<()> {
        let Some(control) = &self.control else {
            return Ok(());
        };
        if control.nodes.is_empty() {
            return Err(mudu_error!(
                ErrorCode::InvalidArgument,
                "control plane node set must be non-empty"
            ));
        }
        let mut control_ids = std::collections::HashSet::new();
        for node in &control.nodes {
            if !control_ids.insert(node.node_id) {
                return Err(mudu_error!(
                    ErrorCode::InvalidArgument,
                    format!("duplicate control plane node {}", node.node_id)
                ));
            }
            // Control nodes are dedicated: they must not also be data
            // group members (the two planes are disjoint).
            if self.group_of_node(node.node_id).is_some() {
                return Err(mudu_error!(
                    ErrorCode::InvalidArgument,
                    format!(
                        "control plane node {} also belongs to a data group",
                        node.node_id
                    )
                ));
            }
        }
        Ok(())
    }

    /// The control-plane voter set: the explicit scale-out control nodes, or
    /// every cluster node in the Combined deployment.
    pub fn control_voters(&self) -> Vec<u64> {
        match &self.control {
            Some(control) => control.nodes.iter().map(|node| node.node_id).collect(),
            None => self
                .group
                .iter()
                .flat_map(|group| group.members.iter().map(|member| member.node_id))
                .collect(),
        }
    }

    /// The control-plane learner set. Phase 2 fully decouples the data plane:
    /// data nodes subscribe through a control client instead of joining the
    /// control Raft group as learners, so the learner set is always empty.
    /// (Phase 1 kept data nodes attached as learners.)
    pub fn control_learners(&self) -> Vec<u64> {
        Vec::new()
    }

    /// Every data-plane node (a member of some data group). These are the
    /// transaction epoch participants: they carry business partitions and must
    /// provide drain evidence before the stable epoch advances. Independent of
    /// the control Raft membership (voters/learners).
    pub fn data_nodes(&self) -> Vec<u64> {
        self.group
            .iter()
            .flat_map(|group| group.members.iter().map(|member| member.node_id))
            .collect()
    }

    /// The control group's cluster namespace: the cluster id derived at
    /// config load from the cluster name (`fnv1a64(cluster)`).
    pub fn control_cluster_id(&self) -> u64 {
        self.cluster_id
    }

    /// The control node with `node_id`, when it is a control member.
    pub fn control_node(&self, node_id: u64) -> Option<&ControlPlaneNode> {
        self.control
            .as_ref()?
            .nodes
            .iter()
            .find(|node| node.node_id == node_id)
    }

    /// Every node of the cluster — data group members and dedicated control
    /// nodes — as `(node_id, host, channel_port)`. Used to build the
    /// full-mesh channel transport.
    pub fn all_peer_nodes(&self) -> Vec<(u64, String, u16)> {
        let mut peers = Vec::new();
        for group in &self.group {
            for member in &group.members {
                peers.push((member.node_id, member.host.clone(), member.channel_port));
            }
        }
        if let Some(control) = &self.control {
            for node in &control.nodes {
                peers.push((node.node_id, node.host.clone(), node.channel_port));
            }
        }
        peers
    }

    /// The data-plane channel address of any node: a data group member, or a
    /// dedicated control node.
    pub fn node_channel_addr(&self, node_id: u64) -> Option<(String, u16)> {
        if let Some(member) = self
            .group_of_node(node_id)
            .and_then(|group| group.member(node_id))
        {
            return Some((member.host.clone(), member.channel_port));
        }
        self.control_node(node_id)
            .map(|node| (node.host.clone(), node.channel_port))
    }

    /// The client address of any node: a data group member, or a dedicated
    /// control node.
    pub fn node_client_addr(&self, node_id: u64) -> Option<(String, u16)> {
        if let Some(member) = self
            .group_of_node(node_id)
            .and_then(|group| group.member(node_id))
        {
            return Some((member.host.clone(), member.client_port));
        }
        self.control_node(node_id)
            .map(|node| (node.host.clone(), node.client_port))
    }

    /// The role of one cluster node derived from the topology.
    pub fn role_of(&self, node_id: u64) -> Option<NodeRole> {
        match &self.control {
            Some(control) => {
                if control.nodes.iter().any(|node| node.node_id == node_id) {
                    Some(NodeRole::ControlPlane)
                } else if self.group_of_node(node_id).is_some() {
                    Some(NodeRole::DataPlane)
                } else {
                    None
                }
            }
            None => {
                self.group_of_node(node_id)?;
                Some(NodeRole::Combined)
            }
        }
    }

    /// Finds a group by id.
    pub fn group(&self, group_id: u64) -> Option<&ClusterGroup> {
        self.group.iter().find(|group| group.group_id == group_id)
    }

    /// Finds the placement of `(rule, partition)`.
    pub fn placement(&self, rule: &str, partition: &str) -> Option<&StaticPlacement> {
        self.placements
            .iter()
            .find(|placement| placement.rule == rule && placement.partition == partition)
    }

    /// Finds the group containing `node_id`.
    pub fn group_of_node(&self, node_id: u64) -> Option<&ClusterGroup> {
        self.group
            .iter()
            .find(|group| group.member(node_id).is_some())
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    fn member(node_id: u64) -> ClusterGroupMember {
        ClusterGroupMember {
            node_id,
            host: "127.0.0.1".to_string(),
            channel_port: 19527,
            client_port: 5432,
        }
    }

    fn cnode(node_id: u64) -> ControlPlaneNode {
        ControlPlaneNode {
            node_id,
            host: "127.0.0.1".to_string(),
            channel_port: 19528,
            client_port: 5432,
        }
    }

    fn topology() -> ClusterTopology {
        ClusterTopology {
            version: 1,
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
            placements: vec![StaticPlacement {
                rule: "r".to_string(),
                partition: "p1".to_string(),
                group_id: 2,
                worker_index: 0,
            }],
            control: None,
            cluster_id: 0,
        }
    }

    #[test]
    fn valid_topology_passes_and_lookups_work() {
        let topology = topology();
        topology.validate().unwrap();
        assert_eq!(topology.group(2).unwrap().members[0].node_id, 3);
        assert_eq!(topology.placement("r", "p1").unwrap().group_id, 2);
        assert_eq!(topology.group_of_node(2).unwrap().group_id, 1);
        assert!(topology.group_of_node(9).is_none());
    }

    #[test]
    fn duplicate_group_id_rejected() {
        let mut topology = topology();
        topology.group.push(topology.group[0].clone());
        assert!(topology.validate().is_err());
    }

    #[test]
    fn shared_node_id_rejected() {
        let mut topology = topology();
        topology.group[1].members.push(member(1));
        assert!(topology.validate().is_err());
    }

    #[test]
    fn placement_reference_errors_rejected() {
        let mut topo = topology();
        topo.placements[0].group_id = 99;
        assert!(topo.validate().is_err());
        let mut topo = topology();
        topo.placements[0].worker_index = 9;
        assert!(topo.validate().is_err());
        let mut topo = topology();
        topo.placements.push(topo.placements[0].clone());
        assert!(topo.validate().is_err());
    }

    #[test]
    fn control_membership_validation_and_roles() {
        let mut topo = topology();
        // Combined deployment: every node is a control voter, no learners.
        assert_eq!(topo.control_voters(), vec![1, 2, 3]);
        assert!(topo.control_learners().is_empty());
        assert_eq!(topo.role_of(1), Some(NodeRole::Combined));
        assert_eq!(topo.role_of(9), None);

        // Scale-out: dedicated control nodes 10/11/12, disjoint from the data
        // groups (1, 2, 3); the data nodes subscribe through a control client,
        // not as Raft learners.
        topo.control = Some(ControlPlaneTopology {
            nodes: vec![cnode(10), cnode(11), cnode(12)],
        });
        topo.validate().unwrap();
        assert_eq!(topo.control_voters(), vec![10, 11, 12]);
        assert!(topo.control_learners().is_empty());
        assert_eq!(topo.data_nodes(), vec![1, 2, 3]);
        assert_eq!(topo.role_of(10), Some(NodeRole::ControlPlane));
        assert_eq!(topo.role_of(1), Some(NodeRole::DataPlane));
        assert_eq!(topo.role_of(9), None);

        // Addresses resolve for control nodes even though they are in no
        // data group.
        assert_eq!(
            topo.node_channel_addr(10),
            Some(("127.0.0.1".to_string(), 19528))
        );
        assert_eq!(topo.control_node(11).map(|n| n.node_id), Some(11));

        let mut invalid = topology();
        invalid.control = Some(ControlPlaneTopology { nodes: vec![] });
        assert!(invalid.validate().is_err());
        // A control node may not also be a data group member.
        let mut invalid = topology();
        invalid.control = Some(ControlPlaneTopology {
            nodes: vec![cnode(1)],
        });
        assert!(invalid.validate().is_err());
        let mut invalid = topology();
        invalid.control = Some(ControlPlaneTopology {
            nodes: vec![cnode(10), cnode(10)],
        });
        assert!(invalid.validate().is_err());
    }
}
