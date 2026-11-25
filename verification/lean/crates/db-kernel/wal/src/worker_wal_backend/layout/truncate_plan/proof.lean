import crates.«db-kernel».wal.src.worker_wal_backend.layout.truncate_plan.spec
open Aeneas Aeneas.Std Result
namespace truncate_plan_proof
open truncate_plan.truncate_plan

/-- Equality is cut too: the boundary is the first LSN to discard. -/
theorem cut_correct (lsn boundary : U64) :
    cut_before lsn boundary ⦃ r => r = true ↔ boundary.val ≤ lsn.val ⦄ := by
  simp [cut_before]

/-- The cut chunk and every earlier chunk survive the removal decision. -/
theorem remove_correct (sequence cut : U64) :
    remove_chunk sequence cut ⦃ r => r = true ↔ cut.val < sequence.val ⦄ := by
  simp [remove_chunk]

/-- A reset preserves the retained byte offset, or retires a full chunk.
The next sequence is always fresh, and exhaustion is explicit. -/
@[step] theorem reset_correct (sequence offset capacity : U64) :
    reset sequence offset capacity ⦃ r => match r with
      | none => sequence.val = 18446744073709551615
      | some (current, size, next) => next.val = sequence.val + 1 ∧
        (if offset.val < capacity.val
         then current = some sequence ∧ size = offset
         else current = none ∧ size.val = 0) ⦄ := by
  unfold reset
  have hm : core.num.U64.MAX = 18446744073709551615#u64 := rfl
  rw [hm]
  split
  · simp_all
  · step
    split <;> simp_all

/-- The cut offset is exactly a sum of complete frames before the first
matching LSN. No later candidate can move the cut past that first match. -/
theorem first_cut (frames : List (U64 × Nat)) (boundary : U64) :
    retainedBytes boundary frames ⦃ n =>
      n = ((frames.takeWhile (fun f => f.1.val < boundary.val)).map Prod.snd).sum ⦄ := by
  induction frames with
  | nil => simp [retainedBytes]
  | cons f rest ih =>
    rcases f with ⟨lsn, size⟩
    unfold retainedBytes cut_before
    by_cases h : lsn.val < boundary.val
    · simp [h, show ¬boundary.val ≤ lsn.val by omega]
      step with ih
      simp_all
    · simp [h, show boundary.val ≤ lsn.val by omega]
/-- Sanity: a concrete frame at the boundary is discarded, one below is retained. -/
theorem sanity_cut :
    cut_before 5#u64 10#u64 ⦃ r => r = false ⦄ := by
  have h := cut_correct 5#u64 10#u64
  apply WP.spec_mono h
  intro r hr
  rw [show (10#u64 : U64).val = 10 from rfl, show (5#u64 : U64).val = 5 from rfl] at hr
  cases r with
  | true => exact absurd (hr.mp rfl) (by decide)
  | false => rfl

/-- Sanity: a concrete reset retains the cut offset with a fresh sequence. -/
theorem sanity_reset :
    reset 7#u64 100#u64 4096#u64 ⦃ r =>
      ∃ current size next, r = some (current, size, next) ∧
        current = some 7#u64 ∧ size = 100#u64 ∧ next.val = 8 ⦄ := by
  have h := reset_correct 7#u64 100#u64 4096#u64
  apply WP.spec_mono h
  intro r hr
  cases r with
  | none => exact absurd hr (by decide)
  | some t =>
    rcases t with ⟨current, size, next⟩
    obtain ⟨hn, hbr⟩ := hr
    rw [show (100#u64 : U64).val = 100 from rfl, show (4096#u64 : U64).val = 4096 from rfl] at hbr
    simp only [show 100 < 4096 by decide, ite_true] at hbr
    rw [show (7#u64 : U64).val = 7 from rfl] at hn
    exact ⟨current, size, next, rfl, hbr.1, hbr.2, by omega⟩

end truncate_plan_proof
