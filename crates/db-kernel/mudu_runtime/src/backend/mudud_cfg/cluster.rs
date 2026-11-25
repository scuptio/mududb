//! Cluster topology configuration: the `control_plane` and `data_plane`
//! sections (design: `doc/en/cluster_configuration.md`).

use super::replication::{RaftReplCfg, StreamCfg};
use serde::{Deserialize, Serialize};

/// A `[[control_plane.replica.member]]` entry: one dedicated control-plane
/// node. Control nodes are disjoint from the data groups; they host the
/// control replica set but no business partitions.
#[derive(Serialize, Deserialize, Eq, PartialEq, Debug, Clone)]
pub struct ControlMemberCfg {
    /// Cluster-global unique name of the control node.
    pub name: String,
    /// Hostname or IP the control node's channel listens on.
    pub host: String,
    /// Control-plane channel port.
    pub channel_port: u16,
    /// Client port of the control node.
    pub client_port: u16,
}

/// The control replica set: always Raft, with its bootstrap members.
#[derive(Serialize, Deserialize, Eq, PartialEq, Debug, Clone)]
pub struct ControlReplicaCfg {
    /// `[control_plane.replica.raft]`: the only allowed control mode.
    #[serde(default)]
    pub raft: Option<RaftReplCfg>,
    /// `[[control_plane.replica.member]]`: control bootstrap voting members.
    #[serde(default)]
    pub member: Vec<ControlMemberCfg>,
}

/// `[control_plane]`: the dedicated control plane. Omitted selects the
/// Combined layout (the control replica set is the single data replica set).
#[derive(Serialize, Deserialize, Eq, PartialEq, Debug, Clone)]
pub struct ControlPlaneCfg {
    /// The control replica set.
    pub replica: ControlReplicaCfg,
}

/// A `[[data_plane.group.replica.member]]` entry.
#[derive(Serialize, Deserialize, Eq, PartialEq, Debug, Clone)]
pub struct DataMemberCfg {
    /// Cluster-global unique name of the member.
    pub name: String,
    /// Hostname or IP the member's data-plane channel listens on.
    pub host: String,
    /// Data-plane channel port of the member.
    pub channel_port: u16,
    /// Client (pgwire) port of the member.
    pub client_port: u16,
    /// Data replication port of the member.
    pub replication_port: u16,
}

/// The data replica set of one group: at most one of `raft` / `stream` may be
/// present, selecting the replication mode. With neither, the group is a
/// single replica (one member, no replication mechanism).
#[derive(Serialize, Deserialize, Eq, PartialEq, Debug, Clone)]
pub struct DataReplicaCfg {
    /// `[data_plane.group.replica.raft]`: Raft mode parameters.
    #[serde(default)]
    pub raft: Option<RaftReplCfg>,
    /// `[data_plane.group.replica.stream]`: stream mode parameters.
    #[serde(default)]
    pub stream: Option<StreamCfg>,
    /// `[[data_plane.group.replica.member]]`: bootstrap members.
    #[serde(default)]
    pub member: Vec<DataMemberCfg>,
}

/// A `[[data_plane.group]]` entry: one data replica set.
#[derive(Serialize, Deserialize, Eq, PartialEq, Debug, Clone)]
pub struct DataGroupCfg {
    /// Unique data-group name.
    pub name: String,
    /// Workers per member node (identical across the group).
    pub worker_count: u32,
    /// The group's replica set (members and mode).
    pub replica: DataReplicaCfg,
}

/// A `[[data_plane.placements]]` entry: static placement of one named
/// partition of a partition rule.
#[derive(Serialize, Deserialize, Eq, PartialEq, Debug, Clone)]
pub struct PlacementCfg {
    /// Name of the partition rule.
    pub rule: String,
    /// Name of the partition within the rule.
    pub partition: String,
    /// Owning data-group name.
    pub group: String,
    /// Worker index within the group's nodes.
    pub worker_index: u32,
}

/// `[data_plane]`: the data groups, static placements and rebalance
/// scheduler.
#[derive(Serialize, Deserialize, Eq, PartialEq, Debug, Clone)]
pub struct DataPlaneCfg {
    /// The data group list: one entry per data replica set (data plane group).
    #[serde(default)]
    pub group: Vec<DataGroupCfg>,
    /// Static partition placements.
    #[serde(default)]
    pub placements: Vec<PlacementCfg>,
    /// Rebalance scheduler round interval in milliseconds; 0 disables it.
    #[serde(default)]
    pub rebalance_interval_ms: u64,
}
