//! Extractable decision core of the Raft quorum arithmetic.
//!
//! The `HashMap`/closure/sort plumbing stays in `quorum.rs`; this module
//! carries the pure decision functions the formal verification registry
//! (`verification/targets.toml`, target `raft_quorum`) extracts and proves.
//!
//! Faithful port of scupt-raft `quorum.rs` (Apache-2.0, scuptio/scupt-raft).

/// Count of agreeing voters, excluding `exclude` (the leader counts
/// separately). `granted` is position-aligned with `voters`.
pub fn count_agreeing_excluding(voters: &[u64], granted: &[bool], exclude: u64) -> usize {
    let mut n = 0;
    let mut i = 0;
    while i < voters.len() {
        if voters[i] != exclude && granted[i] {
            n += 1;
        }
        i += 1;
    }
    n
}

/// Whether the agreed count (the leader already included in it) reaches a
/// strict majority of `voters`.
pub fn majority_met(agreed: usize, voters: usize) -> bool {
    agreed * 2 > voters
}

/// The majority commit index from a descending-sorted list of per-node match
/// indices (the caller pushes the leader's last index in): the
/// `(len/2+1)`-th largest, i.e. `sorted_desc[len/2]`. Returns `None` when the
/// list is not exactly `nodes` long or empty.
pub fn majority_pick(sorted_desc: &[u64], nodes: usize) -> Option<u64> {
    let len = sorted_desc.len();
    if len == nodes && len >= 1 {
        Some(sorted_desc[len / 2])
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn count_excludes_the_leader_and_counts_grants() {
        let voters = [1, 2, 3];
        assert_eq!(count_agreeing_excluding(&voters, &[true, true, true], 1), 2);
        assert_eq!(
            count_agreeing_excluding(&voters, &[false, true, false], 1),
            1
        );
        assert_eq!(count_agreeing_excluding(&voters, &[true, true, true], 4), 3);
        assert_eq!(
            count_agreeing_excluding(&voters, &[false, false, false], 1),
            0
        );
    }

    #[test]
    fn majority_met_is_strict() {
        assert!(majority_met(2, 3));
        assert!(!majority_met(1, 3));
        assert!(!majority_met(2, 4));
        assert!(majority_met(3, 4));
        assert!(majority_met(1, 1));
    }

    #[test]
    fn majority_pick_takes_the_high_median() {
        assert_eq!(majority_pick(&[10, 7, 4], 3), Some(7));
        assert_eq!(majority_pick(&[10, 4, 1], 3), Some(4));
        assert_eq!(majority_pick(&[9], 1), Some(9));
        assert_eq!(majority_pick(&[10, 7], 3), None);
        assert_eq!(majority_pick(&[], 0), None);
    }
}
