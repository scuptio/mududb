import crates.«db-kernel».storage.src.page.sequence_edit.spec
open Aeneas Aeneas.Std Result verification_sequences
namespace sequence_edit_proof
open sequence_edit.sequence_edit

/-- Exact logical edit: the retained prefix and suffix are unchanged, and the
replacement occurs once. Clone must preserve values, as it does for page slots. -/
@[step] theorem splice_correct {T : Type} (c : core.clone.Clone T)
    (hc : ∀ x, c.clone x = ok x) (items replacement : Slice T) (start stop : Usize)
    (horder : start.val ≤ stop.val) (hbound : stop.val ≤ items.val.length)
    (hsize : items.val.length - (stop.val - start.val) + replacement.val.length ≤ Usize.max) :
    splice c items start stop replacement ⦃ r => ∃ output, r = some output ∧
      output.val = items.val.take start.val ++ replacement.val ++ items.val.drop stop.val ⦄ := by
  unfold splice
  simp only [core.num.Usize.MAX]
  simp only [Slice.len]
  split
  · scalar_tac
  · split
    · scalar_tac
    · step
      step
      step
      split
      · scalar_tac
      · step
        step with extend_values (hc := hc)
        step with extend_values (hc := hc)
        step
        step with extend_values (hc := hc)
        simp_all [List.append_assoc]

/-- Publication is all-or-nothing and never changes bytes on length mismatch. -/
@[step] theorem publish_correct (page prepared : Slice U8) :
    publish page prepared ⦃ r =>
      (r.1 = true ∧ r.2 = prepared ∧ page.val.length = prepared.val.length) ∨
      (r.1 = false ∧ r.2 = page ∧ page.val.length ≠ prepared.val.length) ⦄ := by
  unfold publish
  simp only
  split
  · simp_all
  · step
    simp_all

/-- The production comparator implements lexicographic key order. -/
theorem key_order (a b c d : U64) :
    key_less a b c d ⦃ r => r = true ↔ a.val < c.val ∨ (a = c ∧ b.val < d.val) ⦄ := by
  unfold key_less
  split <;> simp_all
  split <;> simp_all

/-- A splice preserves ordering when each part is ordered and the boundary
relations hold. Deletion uses an empty middle; insertion supplies one record. -/
theorem ordered_concat {T : Type} (R : T → T → Prop) (left middle right : List T)
    (hl : left.Pairwise R) (hm : middle.Pairwise R) (hr : right.Pairwise R)
    (hlm : ∀ x ∈ left, ∀ y ∈ middle, R x y)
    (hlr : ∀ x ∈ left, ∀ y ∈ right, R x y)
    (hmr : ∀ x ∈ middle, ∀ y ∈ right, R x y) :
    (left ++ middle ++ right).Pairwise R := by
  simp_all [List.pairwise_append]
  grind

/-- The extracted edit itself returns an ordered sequence when its retained
parts and replacement satisfy the ordering and cross-boundary premises. -/
theorem splice_ordered {T : Type} (c : core.clone.Clone T)
    (hc : ∀ x, c.clone x = ok x) (items replacement : Slice T) (start stop : Usize)
    (horder : start.val ≤ stop.val) (hbound : stop.val ≤ items.val.length)
    (hsize : items.val.length - (stop.val - start.val) + replacement.val.length ≤ Usize.max)
    (R : T → T → Prop)
    (hl : (items.val.take start.val).Pairwise R)
    (hm : replacement.val.Pairwise R) (hr : (items.val.drop stop.val).Pairwise R)
    (hlm : ∀ x ∈ items.val.take start.val, ∀ y ∈ replacement.val, R x y)
    (hlr : ∀ x ∈ items.val.take start.val, ∀ y ∈ items.val.drop stop.val, R x y)
    (hmr : ∀ x ∈ replacement.val, ∀ y ∈ items.val.drop stop.val, R x y) :
    splice c items start stop replacement ⦃ r => ∃ output, r = some output ∧ output.val.Pairwise R ⦄ := by
  step with splice_correct (hc := hc) as ⟨r, output, heq, hval⟩
  refine ⟨output, heq, ?_⟩
  rw [hval]
  exact ordered_concat R _ _ _ hl hm hr hlm hlr hmr

/-- Deletion needs only the original ordering invariant: removing a range
produces a subsequence, so no additional cross-boundary assumptions are needed. -/
theorem delete_ordered {T : Type} (c : core.clone.Clone T)
    (hc : ∀ x, c.clone x = ok x) (items : Slice T) (start stop : Usize)
    (horder : start.val ≤ stop.val) (hbound : stop.val ≤ items.val.length)
    (R : T → T → Prop) (hsorted : items.val.Pairwise R) :
    splice c items start stop (Slice.from [] (by simp)) ⦃ r =>
      ∃ output, r = some output ∧ output.val.Pairwise R ⦄ := by
  have hcapacity : items.val.length - (stop.val - start.val) ≤ Usize.max := by
    have hb := items.property
    omega
  step with splice_correct (hc := hc) (items := items)
    (replacement := Slice.from [] (by simp)) (start := start) (stop := stop)
    (hsize := by simpa using hcapacity) as ⟨r, output, heq, hval⟩
  refine ⟨output, heq, ?_⟩
  rw [hval]
  have sub : List.Sublist (items.val.take start.val ++ items.val.drop stop.val) items.val := by
    have h := List.Sublist.append (List.Sublist.refl (items.val.take start.val))
      (List.drop_sublist_drop_left items.val horder)
    simpa using h
  simpa using List.Pairwise.sublist sub hsorted

/-- The production wrapper publishes only a successfully prepared image.
A failed preparation propagates without evaluating publication. -/
theorem transaction_failure (page : Slice U8) (error : Error) :
    (do let image ← (Result.fail error : Result (Slice U8)); publish page image) = Result.fail error := by
  simp

/-- Each compaction copy preserves the materialized payload exactly and changes
only its planned interval. Existing allocation proofs establish disjoint ranges. -/
theorem compaction_payloads (page source : Slice U8) (offset : Usize)
    (h : offset.val + source.val.length ≤ page.val.length) :
    copy_payload page offset source ⦃ r => r.1 = true ∧
      r.2.val = page.val.take offset.val ++ source.val ++ page.val.drop (offset.val + source.val.length) ⦄ := by
  unfold copy_payload
  simp only
  split
  · scalar_tac
  · step
    split
    · scalar_tac
    · step as ⟨stop, hstop⟩
      step as ⟨region, back, hregion, hlength, hback⟩
      have hlen : region.length = source.length := by
        rw [hlength]
        simp only [Slice.len, Slice.length] at *
        scalar_tac
      step as ⟨copied, hcopy⟩
      rw [hback, hcopy]
      simp [Slice.setSlice!, List.setSlice!, show source.val.length ≤ page.val.length - offset.val by omega]
/-- Sanity: a concrete splice replaces exactly the planned range. -/
theorem sanity_splice :
    splice core.clone.CloneU8 (Aeneas.Std.Slice.from [1#u8, 2#u8, 3#u8, 4#u8] (by scalar_tac))
      1#usize 3#usize (Aeneas.Std.Slice.from [9#u8] (by scalar_tac)) ⦃ r =>
      ∃ output, r = some output ∧ output.val = [1#u8, 9#u8, 4#u8] ⦄ := by
  have h := splice_correct core.clone.CloneU8 (by intro x; rfl)
    (Aeneas.Std.Slice.from [1#u8, 2#u8, 3#u8, 4#u8] (by scalar_tac))
    (Aeneas.Std.Slice.from [9#u8] (by scalar_tac))
    1#usize 3#usize (by decide) (by decide)
    (by simp only [Aeneas.Std.Slice.from_val]; scalar_tac)
  apply WP.spec_mono h
  intro r hr
  obtain ⟨output, heq, hval⟩ := hr
  exact ⟨output, heq, by rw [hval]; rfl⟩

/-- Sanity: a concrete equal-length publication succeeds and swaps the bytes. -/
theorem sanity_publish :
    publish (Aeneas.Std.Slice.from [1#u8, 2#u8] (by scalar_tac))
      (Aeneas.Std.Slice.from [9#u8, 8#u8] (by scalar_tac)) ⦃ r =>
      r.1 = true ∧ r.2 = Aeneas.Std.Slice.from [9#u8, 8#u8] (by scalar_tac) ⦄ := by
  have h := publish_correct (Aeneas.Std.Slice.from [1#u8, 2#u8] (by scalar_tac))
    (Aeneas.Std.Slice.from [9#u8, 8#u8] (by scalar_tac))
  apply WP.spec_mono h
  intro r hr
  cases hr with
  | inl hok => exact ⟨hok.1, hok.2.1⟩
  | inr hno => exact absurd rfl hno.2.2

end sequence_edit_proof
