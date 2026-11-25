import crates.«db-kernel».mudu_raft.src.core.vote_core.spec
open Aeneas Std Result
open raft_vote_spec

namespace raft_vote_proof

/-- The freshness predicate, characterized as a lexicographic comparison. -/
theorem fresh_iff (lt li ct ci : U64) :
    vote_core.vote_core.log_fresh lt li ct ci ⦃ r => (r = true) ↔ fresh lt li ct ci ⦄ := by
  unfold vote_core.vote_core.log_fresh
  split
  · rename_i h
    have h' : ct.val > lt.val := by scalar_tac
    simp [fresh, h']
  · split
    · rename_i h
      have h'' : ct = lt := by scalar_tac
      rw [WP.spec_ok]
      simp [fresh, h'', decide_eq_true_eq]
    · rename_i h
      have hle : ct.val ≤ lt.val := by scalar_tac
      rw [WP.spec_ok]
      simp [fresh, show ¬ ct = lt by scalar_tac, hle]

/-- The vote decision, characterized by the grant predicate. -/
theorem vote_iff (current_term : U64) (voted_for : Option U64) (candidate_term : U64)
    (candidate : U64) (log_fresh_ok : Bool) :
    vote_core.vote_core.vote_decision current_term voted_for candidate_term candidate
      log_fresh_ok ⦃ r => (r = true) ↔
        grants current_term voted_for candidate_term candidate log_fresh_ok ⦄ := by
  unfold vote_core.vote_core.vote_decision
  split
  · rename_i hfresh
    split
    · rename_i hgt
      rw [WP.spec_ok]
      simp [grants, hfresh, show candidate_term.val > current_term.val from by scalar_tac]
    · rename_i hngt
      split
      · rename_i heq
        split
        · rw [WP.spec_ok]
          simp [grants, hfresh, heq]
        · rename_i n
          rw [WP.spec_ok]
          simp [grants, hfresh, heq, decide_eq_true_eq]
      · rename_i hne
        rw [WP.spec_ok]
        simp [grants, hfresh, hne,
          show ¬ candidate_term.val > current_term.val from by scalar_tac]
  · rename_i hnfresh
    rw [WP.spec_ok]
    simp [grants, hnfresh]

/-- Election safety at the pure-predicate level: once a vote for `voted` is
    recorded in the current term, the same term cannot grant a different
    candidate. -/
theorem no_double_grant (current_term voted candidate : U64) (log_fresh_ok : Bool)
    (hne : candidate ≠ voted) :
    vote_core.vote_core.vote_decision current_term (some voted) current_term candidate
      log_fresh_ok ⦃ r => r = false ⦄ := by
  have h := vote_iff current_term (some voted) current_term candidate log_fresh_ok
  apply WP.spec_mono h
  intro r hr
  cases r
  · rfl
  · have hg := hr.mp rfl
    simp only [grants] at hg
    obtain ⟨-, hgt | ⟨-, hv | hv⟩⟩ := hg
    · exact absurd hgt (_root_.lt_irrefl _)
    · exact absurd hv (Option.some_ne_none voted)
    · exact absurd (Option.some.inj hv).symm hne

/-- A granted vote never goes to an older term. -/
theorem grant_term_bound (current_term : U64) (voted_for : Option U64)
    (candidate_term : U64) (candidate : U64) (log_fresh_ok : Bool) :
    vote_core.vote_core.vote_decision current_term voted_for candidate_term candidate
      log_fresh_ok ⦃ r => r = true → current_term.val ≤ candidate_term.val ⦄ := by
  have h := vote_iff current_term voted_for candidate_term candidate log_fresh_ok
  apply WP.spec_mono h
  intro r hr hrt
  have hg := (hr.mp hrt).2
  rcases hg with hgt | ⟨heq, -⟩
  · exact Nat.le_of_lt hgt
  · rw [heq]

/-- Canary: a fresh log and a newer term win the vote. -/
theorem sanity_fresh_grants :
    (do
      let f ← vote_core.vote_core.log_fresh 2#u64 5#u64 3#u64 0#u64
      vote_core.vote_core.vote_decision 2#u64 none 3#u64 8#u64 f) ⦃ r => r = true ⦄ := by
  step with fresh_iff as ⟨f, hf⟩
  have hft : f = true := hf.mpr (Or.inl (by scalar_tac))
  rw [hft]
  have h := vote_iff 2#u64 none 3#u64 8#u64 true
  apply WP.spec_mono h
  intro r hr
  exact hr.mpr ⟨rfl, Or.inl (by scalar_tac)⟩

/-- Canary: a stale log loses the vote even from a newer term. -/
theorem sanity_stale_rejected :
    (do
      let f ← vote_core.vote_core.log_fresh 2#u64 5#u64 1#u64 9#u64
      vote_core.vote_core.vote_decision 2#u64 none 1#u64 8#u64 f) ⦃ r => r = false ⦄ := by
  step with fresh_iff as ⟨f, hf⟩
  have hnf : ¬ fresh 2#u64 5#u64 1#u64 9#u64 := by
    rintro (h | ⟨h, -⟩)
    · scalar_tac
    · exact absurd (congrArg UScalar.val h) (by scalar_tac)
  have hff : f = false := by
    cases hft : f
    · rfl
    · exact absurd (hf.mp hft) hnf
  rw [hff]
  have h := vote_iff 2#u64 none 1#u64 8#u64 false
  apply WP.spec_mono h
  intro r hr
  cases r
  · rfl
  · have hg := hr.mp rfl
    exact Bool.noConfusion hg.1

end raft_vote_proof
