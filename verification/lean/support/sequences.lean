import Aeneas
open Aeneas Aeneas.Std Result
namespace verification_sequences

/-- Cloning is explicit in the contract: the concrete byte and record callers
use value-preserving Clone implementations. Allocation failure is not modeled. -/
@[step] theorem extend_values {T : Type} (c : core.clone.Clone T)
    (hc : ∀ x, c.clone x = ok x) (v : alloc.vec.Vec T) (s : Slice T)
    (hn : v.val.length + s.val.length ≤ Usize.max) :
    alloc.vec.Vec.extend_from_slice c v s ⦃ r => r.val = v.val ++ s.val ⦄ := by
  unfold alloc.vec.Vec.extend_from_slice
  split
  · apply Aeneas.Std.WP.spec_bind (List.clone_spec (by intro x _; exact hc x))
    intro copied h
    simp [h]
  · simp_all
end verification_sequences
