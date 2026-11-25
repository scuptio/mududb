//! Payload durability checks against both sides of joint consensus.

use mudu_raft::ConfNodeValue;
use std::collections::HashSet;

/// Tests the actual voter sets rather than adding an unconditional local vote.
/// Learners and duplicate acknowledgements never contribute extra votes.
pub(crate) fn durable_quorums_met(
    committed: &ConfNodeValue,
    incoming: &ConfNodeValue,
    durable_nodes: &HashSet<u64>,
) -> bool {
    [&committed.node.nid_vote, &incoming.node.nid_vote]
        .into_iter()
        .all(|members| {
            let voters: HashSet<_> = members.iter().copied().collect();
            !voters.is_empty() && voters.intersection(durable_nodes).count() > voters.len() / 2
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use mudu_raft::{ConfValue, NodeInfo};

    fn conf(voters: &[u64], learners: &[u64]) -> ConfNodeValue {
        let nodes = voters
            .iter()
            .map(|id| NodeInfo {
                node_id: *id,
                can_vote: true,
            })
            .chain(learners.iter().map(|id| NodeInfo {
                node_id: *id,
                can_vote: false,
            }))
            .collect();
        ConfNodeValue::new(ConfValue::new(1, 4, nodes), 1, 1, 0)
    }

    #[test]
    fn joint_requires_majorities_of_both_sets() {
        let old = conf(&[1, 2, 3], &[]);
        let new = conf(&[3, 4, 5], &[]);
        assert!(!durable_quorums_met(&old, &new, &HashSet::from([1, 2, 3])));
        assert!(durable_quorums_met(&old, &new, &HashSet::from([1, 3, 4])));
    }

    #[test]
    fn learners_and_nonmember_local_nodes_do_not_vote() {
        let voters = conf(&[2, 3, 4], &[1, 5]);
        assert!(!durable_quorums_met(
            &voters,
            &voters,
            &HashSet::from([1, 2, 5])
        ));
        assert!(durable_quorums_met(
            &voters,
            &voters,
            &HashSet::from([2, 3])
        ));
    }
}
