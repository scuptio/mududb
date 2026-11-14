import crates.«db-kernel».wal.src.part_chain.spec
open Aeneas Aeneas.Std Result verification_sequences
namespace part_chain_proof

/-- A continuation must decrement the counter exactly; completion is possible
only at zero, so a missing, repeated or reordered part is rejected. -/
@[step] theorem advance_correct (previous : Option U32) (remaining : U32) :
    part_chain.part_chain.advance previous remaining ⦃ r =>
      (r = none ↔ ∃ p, previous = some p ∧ p.val ≠ remaining.val + 1) ∧
      (r = some none ↔ remaining.val = 0 ∧ ∀ p, previous = some p → p.val = 1) ∧
      (∀ next, r = some (some next) → next = remaining ∧ 0 < next.val ∧
        ∀ p, previous = some p → p.val = next.val + 1) ⦄ := by
  unfold part_chain.part_chain.advance
  cases previous with
  | none =>
    simp only
    split <;> simp_all
    scalar_tac
  | some p =>
    simp only
    split
    · simp_all
    · step
      split
      · simp_all; scalar_tac
      · split <;> simp_all <;> scalar_tac

/-- Payload accumulation appends exactly the part bytes. A length overflow
returns the original accumulator, with no partial append. -/
@[step] theorem append_correct (payload : alloc.vec.Vec U8) (part : Slice U8) :
    part_chain.part_chain.append payload part ⦃ r =>
      (r.1 = true → r.2.val = payload.val ++ part.val) ∧
      (r.1 = false → r.2 = payload ∧ Usize.max < payload.val.length + part.val.length) ⦄ := by
  unfold part_chain.part_chain.append
  simp only [core.num.Usize.MAX]
  step
  split
  · simp_all; scalar_tac
  · step with extend_values (c := core.clone.CloneU8) (hc := by intro x; rfl)
    simp_all

/-- Two chunk deliveries append the same byte sequence as one combined delivery.
This is the induction step for arbitrary partitioning of a multipart payload. -/
theorem partition_invariant (payload : alloc.vec.Vec U8) (first second : Slice U8)
    (h : payload.val.length + first.val.length + second.val.length ≤ Usize.max) :
    (do let (_, middle) ← part_chain.part_chain.append payload first
        part_chain.part_chain.append middle second) ⦃ r =>
      r.1 = true ∧ r.2.val = payload.val ++ first.val ++ second.val ⦄ := by
  step with append_correct as ⟨accepted, mid, hyes, hno⟩
  have hm : accepted = true := by
    cases hb : accepted <;> simp_all; omega
  have hv := hyes hm
  step with append_correct as ⟨out, hyes2, hno2⟩
  have ho : out.1 = true := by
    cases hb : out.1 <;> simp_all; omega
  exact ⟨ho, by simpa [hv, List.append_assoc] using hyes2 ho⟩
/-- Sanity: a concrete in-order continuation advances to the next counter. -/
theorem sanity_advance :
    part_chain.part_chain.advance (some 3#u32) 2#u32 ⦃ r => r = some (some 2#u32) ⦄ := by
  have h := advance_correct (some 3#u32) 2#u32
  apply WP.spec_mono h
  intro r hr
  obtain ⟨hnone, hdone, hnext⟩ := hr
  cases r with
  | none =>
    obtain ⟨p, hp, hne⟩ := hnone.mp rfl
    have hp' : p = 3#u32 := by simpa using hp.symm
    subst hp'
    rw [show (3#u32 : U32).val = 3 from rfl, show (2#u32 : U32).val = 2 from rfl] at hne
    exact absurd rfl hne
  | some cont =>
    cases cont with
    | none =>
      have h := (hdone.mp rfl).1
      rw [show (2#u32 : U32).val = 2 from rfl] at h
      omega
    | some n => rw [(hnext n rfl).1]

/-- Concrete inputs exercised by the append sanity check. -/
def sanity_payload : alloc.vec.Vec U8 := alloc.vec.Vec.from [1#u8] (by scalar_tac)

/-- Concrete part exercised by the append sanity check. -/
def sanity_part : Slice U8 := Aeneas.Std.Slice.from [2#u8] (by scalar_tac)

/-- Sanity: a concrete append accumulates the part bytes. -/
theorem sanity_append :
    part_chain.part_chain.append sanity_payload sanity_part ⦃ r =>
      r.1 = true ∧ r.2.val = [1#u8, 2#u8] ⦄ := by
  have h := append_correct sanity_payload sanity_part
  apply WP.spec_mono h
  intro r hr
  obtain ⟨hyes, hno⟩ := hr
  by_cases hb : r.1 = true
  · exact ⟨hb, hyes hb⟩
  · obtain ⟨-, hoverflow⟩ := hno (Bool.eq_false_iff.mpr hb)
    have h1 : sanity_payload.val = [1#u8] := rfl
    have h2 : sanity_part.val = [2#u8] := rfl
    rw [h1, h2] at hoverflow
    scalar_tac

end part_chain_proof
