import crates.«db-kernel».mudu_raft.src.core.vote_core.generated
open Aeneas Std Result

namespace raft_vote_spec

/-- Candidate log freshness, lexicographic on (term, index): the candidate's
    last log entry must be at least as recent as the local head. -/
def fresh (lt li ct ci : U64) : Prop :=
  ct.val > lt.val ∨ (ct = lt ∧ ci.val ≥ li.val)

/-- The Raft vote grant predicate over the pure inputs: a fresh log AND
    (a newer term OR (the same term AND not yet voted for another)). -/
def grants (current_term : U64) (voted_for : Option U64) (candidate_term : U64)
    (candidate : U64) (log_fresh_ok : Bool) : Prop :=
  log_fresh_ok = true ∧
    (candidate_term.val > current_term.val ∨
      (candidate_term = current_term ∧ (voted_for = none ∨ voted_for = some candidate)))

end raft_vote_spec
