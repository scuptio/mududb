import crates.«db-kernel».storage.src.page.layout_math.spec
open Aeneas Std Result
namespace page_layout_proof

/-- Slot accounting includes all 32-byte entries and leaves the header intact. -/
theorem slots_correct (tail header count : Usize) :
    layout_math.layout_math.slot_start tail header count ⦃ r =>
      match r with
      | none => tail.val < header.val + count.val * 32
      | some start => header.val ≤ start.val ∧ start.val + count.val * 32 = tail.val ⦄ := by
  unfold layout_math.layout_math.slot_start
  split
  · simp_all; scalar_tac
  · step
    step
    split
    · simp_all; omega
    · step
      step
      simp_all
      scalar_tac

/-- Checked ranges cannot wrap, cross metadata, or exceed their exclusive limit. -/
theorem range_correct (header limit offset size : Usize) :
    layout_math.layout_math.record_end header limit offset size ⦃ r =>
      match r with
      | none => offset.val < header.val ∨ limit.val < offset.val + size.val
      | some stop => header.val ≤ offset.val ∧ stop.val = offset.val + size.val ∧ stop.val ≤ limit.val ⦄ := by
  unfold layout_math.layout_math.record_end
  split
  · simp_all
  · split
    · simp_all; scalar_tac
    · step
      split
      · simp_all; scalar_tac
      · step
        simp_all
        scalar_tac

/-- Allocation consumes payload plus alignment padding; the remainder is exact.
Given earlier records ending at or before `first`, this also proves separation. -/
@[step] theorem allocation_correct (first limit size : Usize) :
    layout_math.layout_math.allocate first limit size ⦃ r =>
      match r with
      | none => limit.val < first.val + (8 - first.val % 8) % 8 + size.val
      | some (start, stop) => first.val ≤ start.val ∧ start.val % 8 = 0 ∧
          stop.val = start.val + size.val ∧ stop.val ≤ limit.val ∧
          (start.val - first.val) + size.val + (limit.val - stop.val) = limit.val - first.val ⦄ := by
  unfold layout_math.layout_math.allocate
  split
  · simp_all; omega
  · step
    step
    step
    step
    split
    · simp_all; omega
    · step
      step
      split
      · simp_all; omega
      · step
        simp_all
        omega
/-- Previously allocated intervals ending before `first` cannot overlap the new record. -/
theorem allocation_separates (first limit size : Usize) (earlierEnd : Nat)
    (h : earlierEnd ≤ first.val) :
    layout_math.layout_math.allocate first limit size ⦃ r =>
      ∀ start stop, r = some (start, stop) → earlierEnd ≤ start.val ∧ stop.val ≤ limit.val ⦄ := by
  step with allocation_correct as ⟨r, hr⟩
  cases r with
  | none => simp_all
  | some pair => rcases pair with ⟨start, stop⟩; simp_all; omega
/-- Sanity: allocation on a concrete unaligned cursor lands at the aligned start. -/
theorem sanity_allocation :
    layout_math.layout_math.allocate 129#usize 140#usize 4#usize ⦃ r =>
      ∃ start stop, r = some (start, stop) ∧ start.val = 136 ∧ stop.val = 140 ⦄ := by
  have h := allocation_correct 129#usize 140#usize 4#usize
  apply WP.spec_mono h
  intro r hr
  cases r with
  | none =>
    rw [show (140#usize : Usize).val = 140 from rfl, show (129#usize : Usize).val = 129 from rfl,
      show (4#usize : Usize).val = 4 from rfl] at hr
    omega
  | some p =>
    rcases p with ⟨start, stop⟩
    obtain ⟨h1, h2, h3, h4, -⟩ := hr
    rw [show (129#usize : Usize).val = 129 from rfl] at h1
    rw [show (4#usize : Usize).val = 4 from rfl] at h3
    rw [show (140#usize : Usize).val = 140 from rfl] at h4
    exact ⟨start, stop, rfl, by omega, by omega⟩

/-- Sanity: a concrete oversized allocation is rejected. -/
theorem sanity_rejects :
    layout_math.layout_math.allocate 129#usize 140#usize 5#usize ⦃ r => r = none ⦄ := by
  have h := allocation_correct 129#usize 140#usize 5#usize
  apply WP.spec_mono h
  intro r hr
  cases r with
  | none => rfl
  | some p =>
    rcases p with ⟨start, stop⟩
    obtain ⟨h1, h2, h3, h4, -⟩ := hr
    rw [show (129#usize : Usize).val = 129 from rfl] at h1
    rw [show (5#usize : Usize).val = 5 from rfl] at h3
    rw [show (140#usize : Usize).val = 140 from rfl] at h4
    omega

end page_layout_proof
