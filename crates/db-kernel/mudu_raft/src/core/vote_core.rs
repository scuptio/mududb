//! Extractable decision core of the Raft vote predicates.
//!
//! The `RaftCore` state plumbing stays in `election.rs`; this module carries
//! the pure predicates the formal verification registry
//! (`verification/targets.toml`, target `raft_vote`) extracts and proves.
//!
//! Faithful port of scupt-raft election vote rules (Apache-2.0,
//! scuptio/scupt-raft).

/// Candidate log freshness: the candidate's `(last_term, last_index)` must be
/// lexicographically at least the local log head.
pub fn log_fresh(
    local_last_term: u64,
    local_last_index: u64,
    cand_last_term: u64,
    cand_last_index: u64,
) -> bool {
    cand_last_term > local_last_term
        || (cand_last_term == local_last_term && cand_last_index >= local_last_index)
}

/// The Raft vote rule: a fresh log AND (a newer term OR (the same term AND
/// not yet voted for a different candidate)).
pub fn vote_decision(
    current_term: u64,
    voted_for: Option<u64>,
    candidate_term: u64,
    candidate: u64,
    log_fresh_ok: bool,
) -> bool {
    log_fresh_ok
        && (candidate_term > current_term
            || (candidate_term == current_term
                && (match voted_for {
                    Some(n) => n == candidate,
                    None => true,
                })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_freshness_is_lexicographic() {
        assert!(log_fresh(2, 5, 3, 0));
        assert!(log_fresh(2, 5, 2, 5));
        assert!(log_fresh(2, 5, 2, 6));
        assert!(!log_fresh(2, 5, 1, 99));
        assert!(!log_fresh(2, 5, 2, 4));
    }

    #[test]
    fn vote_decision_needs_freshness_and_single_vote_per_term() {
        // fresher term grants
        assert!(vote_decision(2, Some(7), 3, 8, true));
        // same term grants the first candidate only
        assert!(vote_decision(2, None, 2, 8, true));
        assert!(vote_decision(2, Some(8), 2, 8, true));
        assert!(!vote_decision(2, Some(7), 2, 8, true));
        // stale term never grants
        assert!(!vote_decision(2, None, 1, 8, true));
        // stale log never grants
        assert!(!vote_decision(2, None, 3, 8, false));
    }
}
