import crates.«db-kernel».wal.src.worker_wal_backend.append_plan.spec
open Aeneas Std Result
namespace append_plan_proof
open append_plan.append_plan

/-- Every successful reservation preserves the cursor invariant; exhaustion
is returned as data and the Rust caller has not modified its state. -/
@[step] theorem reservation_correct (s : AppendState) (capacity length : U64)
    (hv : Valid s capacity) :
    reserve s capacity length ⦃ r =>
      match r with
      | none => s.next.val = 18446744073709551615 ∧ length.val > 0
      | some p => Reserved s capacity length p ⦄ := by
  unfold reserve
  have hm : core.num.U64.MAX = 18446744073709551615#u64 := rfl
  simp only [hm]
  unfold Valid at hv
  split
  · cases hc : s.current <;> simp_all [Reserved, Valid]
  · split
    · cases hc : s.current
      · simp only
        split
        · simp_all; scalar_tac
        · step
          split <;> simp_all [Reserved, Valid]
      · simp only
        split
        · step
          split
          · step
            split <;> simp_all [Reserved, Valid]
            scalar_tac
          · split
            · simp_all; scalar_tac
            · step
              split <;> simp_all [Reserved, Valid]
        · simp_all
    · split
      · simp_all; scalar_tac
      · step
        split <;> simp_all [Reserved, Valid]
        scalar_tac

/-- A successful positive reservation never overwrites earlier bytes in the
current chunk; allocations in a fresh chunk use a strictly newer sequence. -/
theorem reservation_separates (s : AppendState) (capacity length seq : U64)
    (hv : Valid s capacity) (hc : s.current = some seq) (hl : 0 < length.val) :
    reserve s capacity length ⦃ r => match r with
      | none => True
      | some p => (p.sequence = seq ∧ s.size.val ≤ p.offset.val) ∨ seq.val < p.sequence.val ⦄ := by
  step with reservation_correct as ⟨r, hr⟩
  cases r with
  | none => trivial
  | some p =>
    rcases (hr.2.2.2 hl).1 with h | h
    · left; simp_all
    · right; simpa [Valid, hc, h.1] using hv.2

/-- Composition: two successful positive reservations occupy different chunks
in increasing order, or disjoint half-open intervals within the same chunk. -/
theorem successive_disjoint (s : AppendState) (capacity first second : U64)
    (p q : AppendPlan) (hv : Valid s capacity)
    (hfirst : 0 < first.val) (hsecond : 0 < second.val)
    (hp : Reserved s capacity first p) (hq : Reserved p.state capacity second q) :
    p.sequence.val < q.sequence.val ∨
      (p.sequence = q.sequence ∧ p.offset.val + first.val ≤ q.offset.val) := by
  have before : p.sequence.val < p.state.next.val := by
    rcases (hp.2.2.2 hfirst).1 with reused | fresh
    · have old := hv.2
      rw [reused.1] at old
      simpa [reused.2.2.1] using old
    · rw [fresh.1, fresh.2.2]; omega
  rcases (hq.2.2.2 hsecond).1 with reused | fresh
  · right
    have cursor := (hp.2.2.2 hfirst).2.2
    split at cursor
    · simp [cursor.1] at reused
    · have seqeq : p.sequence = q.sequence := by
        rw [cursor.1] at reused
        exact Option.some.inj reused.1
      exact ⟨seqeq, by rw [reused.2.1]; omega⟩
  · left; simpa [fresh.1] using before
/-- Sanity: a concrete in-chunk reservation succeeds within capacity. -/
theorem sanity_reservation :
    reserve ⟨some 1#u64, 100#u64, 5#u64⟩ 4096#u64 50#u64 ⦃ r =>
      ∃ p, r = some p ∧ p.offset.val + 50 ≤ 4096 ⦄ := by
  have hv : Valid ⟨some 1#u64, 100#u64, 5#u64⟩ 4096#u64 := by
    refine ⟨?_, ?_⟩ <;> decide
  have h := reservation_correct ⟨some 1#u64, 100#u64, 5#u64⟩ 4096#u64 50#u64 hv
  apply WP.spec_mono h
  intro r hr
  cases r with
  | none => exact absurd hr.1 (by decide)
  | some p =>
    obtain ⟨-, -, -, hpos⟩ := hr
    obtain ⟨-, hfits, -⟩ := hpos (by decide)
    exact ⟨p, rfl, hfits (by decide)⟩

/-- Sanity: sequence exhaustion returns none on a concrete exhausted cursor. -/
theorem sanity_exhaustion :
    reserve ⟨none, 0#u64, 18446744073709551615#u64⟩ 4096#u64 50#u64 ⦃ r => r = none ⦄ := by
  have hv : Valid ⟨none, 0#u64, 18446744073709551615#u64⟩ 4096#u64 := by
    refine ⟨?_, ?_⟩ <;> decide
  apply WP.spec_mono (reservation_correct ⟨none, 0#u64, 18446744073709551615#u64⟩ 4096#u64 50#u64 hv)
  intro r hr
  cases r with
  | none => rfl
  | some p =>
    obtain ⟨-, -, -, hpos⟩ := hr
    obtain ⟨hbranch, -, -⟩ := hpos (by decide)
    cases hbranch with
    | inl hre => simp at hre
    | inr hfr =>
      obtain ⟨-, -, hnext⟩ := hfr
      rw [show (18446744073709551615#u64 : U64).val = 18446744073709551615 from rfl] at hnext
      scalar_tac

end append_plan_proof
