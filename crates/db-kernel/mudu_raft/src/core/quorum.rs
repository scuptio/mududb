//! Quorum arithmetic for elections, commit advancement and re-configuration.
//!
//! Faithful port of scupt-raft `quorum.rs` (Apache-2.0, scuptio/scupt-raft).

use std::cmp::min;
use std::collections::HashMap;

use crate::core::conf::{ConfNode, ConfVersion, NID};
use crate::core::message::TermIndex;

/// Whether the node `leader_nid` has collected votes for `current_term` from
/// a quorum. With a joint configuration (committed != new), a quorum is
/// required in *both* configurations.
pub fn quorum_agree_vote(
    leader_nid: NID,
    current_term: u64,
    follower_vote_granted: &HashMap<NID, u64>,
    conf_committed: &ConfNode,
    conf_new: &ConfNode,
) -> bool {
    let voter = &conf_committed.nid_vote;
    let filter = |id: NID| match follower_vote_granted.get(&id) {
        Some(term) => *term == current_term,
        None => false,
    };
    let agreed_num = count(voter, leader_nid, filter) + 1;
    if conf_committed.conf_version().eq(conf_new.conf_version()) {
        agreed_num * 2 > voter.len()
    } else {
        let voter_new = &conf_new.nid_vote;
        let agreed_num_new = count(voter_new, leader_nid, filter) + 1;
        agreed_num * 2 > voter.len() && agreed_num_new * 2 > voter_new.len()
    }
}

/// The highest log index replicated on a quorum (leader included).
/// With a joint configuration, the minimum of the two quorums wins.
pub fn quorum_agree_match_index(
    leader_nid: NID,
    leader_last_index: u64,
    follower_match_index: &HashMap<NID, u64>,
    conf_committed: &ConfNode,
    conf_new: &ConfNode,
) -> u64 {
    let filter = |id: NID| match follower_match_index.get(&id) {
        Some(i) => *i,
        None => 0,
    };
    let quorum_agreed = {
        let node = &conf_committed.nid_vote;
        let mut agreed_index = index_of(node, leader_nid, filter);
        agreed_index.push(leader_last_index);
        majority_agree_index(agreed_index, node.len())
    };
    if conf_committed.conf_version().eq(conf_new.conf_version()) {
        quorum_agreed
    } else {
        let node_new = &conf_new.nid_vote;
        let mut agreed_index_new = index_of(node_new, leader_nid, filter);
        agreed_index_new.push(leader_last_index);
        let quorum_agreed_new = majority_agree_index(agreed_index_new, node_new.len());
        min(quorum_agreed, quorum_agreed_new)
    }
}

/// Whether a quorum of `nid_set` (leader included) has accepted the
/// configuration version `term_version`.
pub fn quorum_check_conf_term_version(
    leader_nid: NID,
    nid_set: &[NID],
    term_version: &ConfVersion,
    follower_conf: &HashMap<NID, ConfVersion>,
) -> bool {
    let filter = |n: NID| match follower_conf.get(&n) {
        Some(tv) => *tv == *term_version,
        None => false,
    };
    let agreed_num = count(nid_set, leader_nid, filter) + 1;
    agreed_num * 2 > nid_set.len()
}

/// Whether a quorum of `nid_set` (leader included) has reached the leader's
/// term and committed at least `leader_conf_commit_index`.
pub fn quorum_check_term_commit_index(
    leader_nid: NID,
    nid_set: &[NID],
    leader_term: u64,
    leader_conf_commit_index: u64,
    follower_term_and_committed_index: &HashMap<NID, TermIndex>,
) -> bool {
    let filter = |n: NID| match follower_term_and_committed_index.get(&n) {
        Some(term_index) => {
            term_index.term == leader_term && term_index.index >= leader_conf_commit_index
        }
        None => false,
    };
    let agreed_num = count(nid_set, leader_nid, filter) + 1;
    agreed_num * 2 > nid_set.len()
}

fn count<F: Fn(NID) -> bool>(node: &[NID], exclude: NID, cond: F) -> usize {
    let mut n = 0;
    for id in node {
        if *id != exclude && cond(*id) {
            n += 1
        }
    }
    n
}

fn index_of<F: Fn(NID) -> u64>(node: &[NID], exclude: NID, get_index: F) -> Vec<u64> {
    let mut vec = vec![];
    for id in node {
        if *id != exclude {
            vec.push(get_index(*id))
        }
    }
    vec
}

fn majority_agree_index(vec: Vec<u64>, nodes_num: usize) -> u64 {
    let mut agreed_index = vec;
    agreed_index.sort_by(|x, y| x.cmp(y).reverse());
    let len = agreed_index.len();
    if len == nodes_num && len >= 1 {
        let n = len / 2 + 1;
        assert!(n * 2 > len); // reach majority
        agreed_index[n - 1]
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_majority_agree_index() {
        let v1 = vec![1, 2, 3];
        assert_eq!(0, majority_agree_index(v1.clone(), 4));
        assert_eq!(2, majority_agree_index(v1.clone(), 3));
        assert_eq!(3, majority_agree_index(vec![3, 3, 3], 3));
        assert_eq!(9, majority_agree_index(vec![9], 1));
    }

    #[test]
    fn vote_quorum_single_node() {
        let conf = ConfNode {
            conf_version: ConfVersion {
                term: 0,
                version: 1,
                index: 0,
            },
            nid_vote: vec![1],
            nid_log: vec![1],
        };
        assert!(quorum_agree_vote(1, 5, &HashMap::new(), &conf, &conf));
    }

    #[test]
    fn vote_quorum_three_nodes() {
        let conf = ConfNode {
            conf_version: ConfVersion {
                term: 0,
                version: 1,
                index: 0,
            },
            nid_vote: vec![1, 2, 3],
            nid_log: vec![1, 2, 3],
        };
        let mut granted = HashMap::new();
        assert!(!quorum_agree_vote(1, 5, &granted, &conf, &conf));
        granted.insert(2, 5);
        assert!(quorum_agree_vote(1, 5, &granted, &conf, &conf));
        // stale-term grant does not count
        let mut stale = HashMap::new();
        stale.insert(2, 4);
        stale.insert(3, 4);
        assert!(!quorum_agree_vote(1, 5, &stale, &conf, &conf));
    }

    #[test]
    fn match_index_quorum_three_nodes() {
        let conf = ConfNode {
            conf_version: ConfVersion {
                term: 0,
                version: 1,
                index: 0,
            },
            nid_vote: vec![1, 2, 3],
            nid_log: vec![1, 2, 3],
        };
        let mut matches = HashMap::new();
        matches.insert(2, 4);
        matches.insert(3, 7);
        // sorted desc [10(leader), 7, 4] -> median 7
        assert_eq!(quorum_agree_match_index(1, 10, &matches, &conf, &conf), 7);
        matches.insert(3, 1);
        // sorted desc [10, 4, 1] -> median 4
        assert_eq!(quorum_agree_match_index(1, 10, &matches, &conf, &conf), 4);
    }
}
