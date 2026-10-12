//! Versioned cluster configuration (joint-consensus style).
//!
//! Port of scupt-raft `conf_version.rs`, `conf_node.rs`, `conf_value.rs`,
//! `node_info.rs` and `raft_conf.rs` (Apache-2.0, scuptio/scupt-raft),
//! trimmed to the fields the consensus core needs.

use std::cmp::Ordering;
use std::hash::{Hash, Hasher};

use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use serde::{Deserialize, Serialize};

/// Node identifier.
pub type NID = u64;

/// `[term, version, index]` triple of a configuration setting.
///
/// TLA+ {D240508N-_ConfVersion}. Equality, hashing and ordering use only
/// `(term, version)`; `index` is informational (the log commit index at the
/// time of the re-configuration).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ConfVersion {
    /// Term in which this configuration was created.
    pub term: u64,
    /// Monotonic configuration version inside a term lineage.
    pub version: u64,
    /// The commit index of the log when the re-configuration began.
    pub index: u64,
}

impl Hash for ConfVersion {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.term.hash(state);
        self.version.hash(state);
    }
}

impl PartialOrd for ConfVersion {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ConfVersion {
    fn cmp(&self, other: &Self) -> Ordering {
        let ord = self.term.cmp(&other.term);
        if ord.is_eq() {
            self.version.cmp(&other.version)
        } else {
            ord
        }
    }
}

impl PartialEq for ConfVersion {
    fn eq(&self, other: &Self) -> bool {
        self.term.eq(&other.term) && self.version.eq(&other.version)
    }
}

impl Eq for ConfVersion {}

/// One member of the cluster configuration.
#[derive(Clone, Hash, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct NodeInfo {
    /// Node id.
    pub node_id: NID,
    /// Whether the node votes (voters are a subset of log-replicating nodes).
    pub can_vote: bool,
}

/// The static part of a configuration: cluster membership plus tunables that
/// travel with the configuration when it is propagated to followers.
#[derive(Clone, Hash, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct ConfValue {
    /// Node id of the local node this configuration belongs to.
    pub node_id: NID,
    /// Election timeout counted in ticks (no wall clock in the core).
    pub timeout_max_tick: u64,
    /// Cluster membership.
    pub node_peer: Vec<NodeInfo>,
}

impl ConfValue {
    /// Creates a configuration value for `node_id` with the given election
    /// timeout (in ticks) and voter set.
    pub fn new(node_id: NID, timeout_max_tick: u64, node_peer: Vec<NodeInfo>) -> Self {
        Self {
            node_id,
            timeout_max_tick,
            node_peer,
        }
    }
}

/// The node sets of one configuration version.
///
/// TLA+ {D240508N-_ConfNode}: `nid_vote` is a subset of `nid_log`.
#[derive(Clone, Hash, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct ConfNode {
    /// Version of this configuration.
    pub conf_version: ConfVersion,
    /// Node set which can vote; `nid_vote` is a subset of `nid_log`.
    pub nid_vote: Vec<NID>,
    /// Node set which replicates the log.
    pub nid_log: Vec<NID>,
}

impl ConfNode {
    /// The version of this configuration node set.
    pub fn conf_version(&self) -> &ConfVersion {
        &self.conf_version
    }
}

/// A configuration node set paired with its membership value.
#[derive(Clone, Hash, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct ConfNodeValue {
    /// Versioned node sets.
    pub node: ConfNode,
    /// Membership value.
    pub value: ConfValue,
}

impl ConfNodeValue {
    /// Builds a `ConfNodeValue` from a membership value and the
    /// `(term, version, commit_index)` triple of the configuration.
    pub fn new(conf_value: ConfValue, term: u64, version: u64, commit_index: u64) -> Self {
        let mut nid_vote = vec![];
        let mut nid_log = vec![];
        for peer in conf_value.node_peer.iter() {
            if peer.can_vote {
                nid_vote.push(peer.node_id);
            }
            nid_log.push(peer.node_id);
        }
        ConfNodeValue {
            node: ConfNode {
                conf_version: ConfVersion {
                    term,
                    version,
                    index: commit_index,
                },
                nid_vote,
                nid_log,
            },
            value: conf_value,
        }
    }

    /// Rebuilds a `ConfNodeValue` from parts (used by recovery).
    pub fn from_conf_node_value(conf_node: ConfNode, conf_value: ConfValue) -> Self {
        Self {
            node: conf_node,
            value: conf_value,
        }
    }

    /// Ids of the voting nodes.
    pub fn nid_vote(&self) -> &Vec<NID> {
        &self.node.nid_vote
    }

    /// The `(term, version, index)` triple of this configuration.
    pub fn term_version(&self) -> &ConfVersion {
        &self.node.conf_version
    }

    /// Derives the `(conf_value, nid_vote, nid_log)` triple for a
    /// re-configuration that adds `node_id` as a voter.
    ///
    /// The result feeds `RaftCore::step_re_conf_begin` /
    /// `RaftHandle::update_conf`. Returns `ErrorCode::InvalidArgument`
    /// when the node is already a member.
    pub fn derive_add_voter(&self, node_id: NID) -> RS<(ConfValue, Vec<NID>, Vec<NID>)> {
        self.derive_add(node_id, true)
    }

    /// Derives the triple for a re-configuration that adds `node_id` as a
    /// learner (replicates the log but never votes).
    ///
    /// Adding as learner first and promoting with
    /// [`ConfNodeValue::derive_promote_voter`] after the learner has caught
    /// up is the safe two-step join.
    pub fn derive_add_learner(&self, node_id: NID) -> RS<(ConfValue, Vec<NID>, Vec<NID>)> {
        self.derive_add(node_id, false)
    }

    /// Derives the triple for a re-configuration that removes `node_id`.
    ///
    /// Returns `ErrorCode::InvalidArgument` when the node is not a member
    /// or when the removal would leave the group without voters.
    pub fn derive_remove_node(&self, node_id: NID) -> RS<(ConfValue, Vec<NID>, Vec<NID>)> {
        let mut value = self.value.clone();
        let before = value.node_peer.len();
        value.node_peer.retain(|p| p.node_id != node_id);
        if value.node_peer.len() == before {
            return Err(mudu_error!(
                ErrorCode::InvalidArgument,
                format!("node {node_id} is not a member of the configuration")
            ));
        }
        if !value.node_peer.iter().any(|p| p.can_vote) {
            return Err(mudu_error!(
                ErrorCode::InvalidArgument,
                "removing the node would leave the group without voters"
            ));
        }
        Ok(Self::derive_sets(value))
    }

    /// Derives the triple for a re-configuration that promotes `node_id`
    /// from learner to voter.
    ///
    /// Returns `ErrorCode::InvalidArgument` when the node is not a learner
    /// of the configuration.
    pub fn derive_promote_voter(&self, node_id: NID) -> RS<(ConfValue, Vec<NID>, Vec<NID>)> {
        let mut value = self.value.clone();
        let peer = value
            .node_peer
            .iter_mut()
            .find(|p| p.node_id == node_id)
            .ok_or_else(|| {
                mudu_error!(
                    ErrorCode::InvalidArgument,
                    format!("node {node_id} is not a member of the configuration")
                )
            })?;
        if peer.can_vote {
            return Err(mudu_error!(
                ErrorCode::InvalidArgument,
                format!("node {node_id} is already a voter")
            ));
        }
        peer.can_vote = true;
        Ok(Self::derive_sets(value))
    }

    fn derive_add(&self, node_id: NID, can_vote: bool) -> RS<(ConfValue, Vec<NID>, Vec<NID>)> {
        if self.value.node_peer.iter().any(|p| p.node_id == node_id) {
            return Err(mudu_error!(
                ErrorCode::InvalidArgument,
                format!("node {node_id} is already a member of the configuration")
            ));
        }
        let mut value = self.value.clone();
        value.node_peer.push(NodeInfo { node_id, can_vote });
        Ok(Self::derive_sets(value))
    }

    /// Recomputes the `(nid_vote, nid_log)` sets from the peer list of
    /// `value` (sorted, deduplicated).
    fn derive_sets(value: ConfValue) -> (ConfValue, Vec<NID>, Vec<NID>) {
        let mut nid_vote = vec![];
        let mut nid_log = vec![];
        for peer in value.node_peer.iter() {
            if peer.can_vote {
                nid_vote.push(peer.node_id);
            }
            nid_log.push(peer.node_id);
        }
        nid_vote.sort_unstable();
        nid_vote.dedup();
        nid_log.sort_unstable();
        nid_log.dedup();
        (value, nid_vote, nid_log)
    }
}

/// The raft configuration: a committed configuration plus the new
/// configuration being jointly agreed on.
///
/// When no re-configuration is in progress, both versions are equal.
#[derive(Clone, Hash, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct RaftConf {
    conf_committed: ConfNodeValue,
    conf_new: ConfNodeValue,
}

impl RaftConf {
    /// The new (possibly joint) configuration.
    pub fn conf_node_value_new(&self) -> &ConfNodeValue {
        &self.conf_new
    }

    /// The committed configuration.
    pub fn conf_node_value_committed(&self) -> &ConfNodeValue {
        &self.conf_committed
    }

    /// Replaces the committed configuration.
    pub fn update_conf_committed(&mut self, conf: ConfNodeValue) {
        self.conf_committed = conf;
    }

    /// Replaces the new configuration.
    pub fn update_conf_new(&mut self, conf: ConfNodeValue) {
        self.conf_new = conf;
    }

    /// Whether a re-configuration is in progress.
    pub fn is_reconfig_ongoing(&self) -> bool {
        !self
            .conf_committed
            .term_version()
            .eq(self.conf_new.term_version())
    }

    /// Value of the committed configuration.
    pub fn conf_committed_value(&self) -> &ConfValue {
        &self.conf_committed.value
    }

    /// Value of the new configuration.
    pub fn conf_new_value(&self) -> &ConfValue {
        &self.conf_new.value
    }

    /// Local node id.
    pub fn node_id(&self) -> NID {
        self.conf_committed.value.node_id
    }

    /// Node sets of the committed configuration.
    pub fn conf_node_committed(&self) -> &ConfNode {
        &self.conf_committed.node
    }

    /// Node sets of the new configuration.
    pub fn conf_node_new(&self) -> &ConfNode {
        &self.conf_new.node
    }

    /// Voting nodes of the committed configuration.
    pub fn conf_committed_nid_vote(&self) -> &Vec<NID> {
        &self.conf_committed.node.nid_vote
    }

    /// Voting nodes of the new configuration.
    pub fn conf_new_nid_vote(&self) -> &Vec<NID> {
        &self.conf_new.node.nid_vote
    }

    /// Log-replicating nodes of the committed configuration.
    pub fn conf_committed_nid_log(&self) -> &Vec<NID> {
        &self.conf_committed.node.nid_log
    }

    /// Log-replicating nodes of the new configuration.
    pub fn conf_new_nid_log(&self) -> &Vec<NID> {
        &self.conf_new.node.nid_log
    }

    /// Overrides the election timeout of both configurations.
    ///
    /// The timeout is a node-local tunable (it is never quorum-relevant), so
    /// the override is not version-stamped.
    pub fn override_timeout_max_tick(&mut self, timeout_max_tick: u64) {
        self.conf_committed.value.timeout_max_tick = timeout_max_tick;
        self.conf_new.value.timeout_max_tick = timeout_max_tick;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conf_version_compares_term_then_version() {
        let a = ConfVersion {
            term: 1,
            version: 2,
            index: 100,
        };
        let b = ConfVersion {
            term: 1,
            version: 3,
            index: 0,
        };
        assert!(a < b);
        assert_ne!(a, b);
        let c = ConfVersion {
            term: 1,
            version: 2,
            index: 55,
        };
        // index is not part of equality
        assert_eq!(a, c);
    }

    #[test]
    fn conf_node_value_splits_voters() {
        let value = ConfValue::new(
            1,
            10,
            vec![
                NodeInfo {
                    node_id: 1,
                    can_vote: true,
                },
                NodeInfo {
                    node_id: 2,
                    can_vote: false,
                },
            ],
        );
        let cnv = ConfNodeValue::new(value, 0, 1, 0);
        assert_eq!(cnv.nid_vote(), &vec![1]);
        assert_eq!(cnv.node.nid_log, vec![1, 2]);
    }
}
