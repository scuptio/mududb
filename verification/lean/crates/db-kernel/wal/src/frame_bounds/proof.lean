import crates.«db-kernel».wal.src.frame_bounds.spec
open Aeneas Std Result
namespace frame_bounds_proof
open frame_bounds.frame_bounds

/-- Payload and trailer intervals fit exactly; all arithmetic is nonwrapping. -/
@[step] theorem bounds_correct (available payload : Usize) :
    bounds available payload ⦃ r => match r with
      | none => available.val < 32 + payload.val
      | some (bodyEnd, frameEnd) =>
        bodyEnd.val = 24 + payload.val ∧ frameEnd.val = bodyEnd.val + 8 ∧
        24 ≤ bodyEnd.val ∧ frameEnd.val ≤ available.val ⦄ := by
  unfold bounds
  split
  · simp_all; omega
  · step
    split
    · simp_all; omega
    · step
      step
      simp_all; omega

/-- The cursor advances by exactly one accepted frame, otherwise it stops at
its old offset. This also establishes progress for every successful step. -/
@[step] theorem advance_correct (p : Prefix) (total length : Usize) (valid : Bool) :
    Prefix.advance p total length valid ⦃ q =>
      (Accept p total length valid →
        q.stopped = false ∧ q.offset.val = p.offset.val + length.val ∧ q.offset.val ≤ total.val) ∧
      (¬Accept p total length valid → q.stopped = true ∧ q.offset = p.offset) ⦄ := by
  unfold Prefix.advance
  cases hs : p.stopped <;> cases hv : valid <;> simp_all [Accept]
  split
  · simp_all
  · split
    · simp_all; omega
    · step
      split
      · simp_all; omega
      · step
        simp_all; omega

/-- Rejecting one frame is irreversible, even if later candidates validate. -/
theorem stopped_stays (p : Prefix) (total length : Usize) (valid : Bool)
    (h : p.stopped = true) :
    Prefix.advance p total length valid ⦃ q => q = p ⦄ := by
  simp [Prefix.advance, h]

/-- Retained bytes are exactly the sum of accepted frame lengths. Together with
`stopped_stays`, this lifts the production step to arbitrary finite scans. -/
theorem prefix_conservation (p : Prefix) (total : Usize) (items : List (Usize × Bool)) :
    scan p total items ⦃ q =>
      q.offset.val = p.offset.val + (acceptedLengths p.offset.val total.val p.stopped items).sum ⦄ := by
  induction items generalizing p with
  | nil => simp [scan, acceptedLengths]
  | cons item rest ih =>
    rcases item with ⟨length, valid⟩
    unfold scan
    step with advance_correct as ⟨q, hAccept, hReject⟩
    step with ih as ⟨r, hr⟩
    by_cases ha : Accept p total length valid
    · have h := hAccept ha.1 ha.2.1 ha.2.2.1 ha.2.2.2
      change p.stopped = false ∧ valid = true ∧ 0 < length.val ∧ p.offset.val + length.val ≤ total.val at ha
      simp only [acceptedLengths, ite_eq_left ha, List.sum_cons]
      rw [h.1, h.2.1] at hr
      omega
    · have hq := hReject ha
      have stopEmpty : ∀ xs, acceptedLengths q.offset.val total.val true xs = [] := by
        intro xs; cases xs <;> simp [acceptedLengths]
      simp [hq.1, stopEmpty] at hr
      change ¬(p.stopped = false ∧ valid = true ∧ 0 < length.val ∧ p.offset.val + length.val ≤ total.val) at ha
      simp only [acceptedLengths, ite_eq_right ha, List.sum_nil, Nat.add_zero]
      simpa [hq.2] using hr
/-- Sanity: concrete frame bounds land at header-plus-payload and plus-trailer. -/
theorem sanity_bounds :
    bounds 100#usize 10#usize ⦃ r =>
      ∃ bodyEnd frameEnd, r = some (bodyEnd, frameEnd) ∧ bodyEnd.val = 34 ∧ frameEnd.val = 42 ⦄ := by
  have h := bounds_correct 100#usize 10#usize
  apply WP.spec_mono h
  intro r hr
  cases r with
  | none =>
    rw [show (100#usize : Usize).val = 100 from rfl, show (10#usize : Usize).val = 10 from rfl] at hr
    omega
  | some p =>
    rcases p with ⟨bodyEnd, frameEnd⟩
    obtain ⟨hb, hf, -, -⟩ := hr
    rw [show (10#usize : Usize).val = 10 from rfl] at hb
    exact ⟨bodyEnd, frameEnd, rfl, by omega, by omega⟩

/-- Sanity: a concrete accepted frame advances the cursor by its length. -/
theorem sanity_advance :
    Prefix.advance ⟨10#usize, false⟩ 100#usize 20#usize true ⦃ q =>
      q.stopped = false ∧ q.offset.val = 30 ⦄ := by
  have h := advance_correct ⟨10#usize, false⟩ 100#usize 20#usize true
  apply WP.spec_mono h
  intro q hq
  obtain ⟨hs, ho, -⟩ := hq.1 ⟨rfl, rfl, by decide, by decide⟩
  rw [show (10#usize : Usize).val = 10 from rfl, show (20#usize : Usize).val = 20 from rfl] at ho
  exact ⟨hs, by omega⟩

end frame_bounds_proof
