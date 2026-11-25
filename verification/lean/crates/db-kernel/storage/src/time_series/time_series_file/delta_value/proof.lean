import crates.«db-kernel».storage.src.time_series.time_series_file.delta_value.spec
open Aeneas Aeneas.Std Result
namespace delta_value_proof
open delta_value.delta_value

/-- A sound equality result permits omission of an unchanged payload; every
other case reproduces the new value, including deletion and insertion. -/
@[step] theorem diff_apply {T : Type} (old new : Option T) (equal : Bool)
    (heq : ∀ a b, old = some a → new = some b → (equal = true ↔ a = b)) :
    (do let change ← diff old.isSome new equal; apply old change) ⦃ r => r = new ⦄ := by
  cases old <;> cases new <;> cases equal <;> simp_all [diff, apply]

/-- A change to one key leaves every other key unchanged. -/
theorem other_keys {K T : Type} [DecidableEq K] (page : K → Option T)
    (key query : K) (change : Change T) (h : query ≠ key) :
    applyAt page key change query = ok (page query) := by
  simp [applyAt, h]

/-- Pointwise lifting: applying the generated delta reconstructs the logical
new page for every key. Sorted merge traversal is covered by Rust regression. -/
theorem page_roundtrip {K T : Type} [DecidableEq T]
    (old new : K → Option T) (key : K) :
    (do let change ← diff (old key).isSome (new key) (decide (old key = new key))
        apply (old key) change) ⦃ r => r = new key ⦄ := by
  apply diff_apply
  intro a b ha hb
  simp [ha, hb]

/-- Replay is skipped exactly for already-applied or newer page images. -/
theorem replay_correct (page batch : U64) :
    replay_needed page batch ⦃ r => r = true ↔ page.val < batch.val ⦄ := by
  simp [replay_needed]

/-- Replaying a successfully stamped batch twice is the same as once, even
for a deletion that would fail if the page primitive were invoked again. -/
theorem replay_idempotent {T : Type} (lsn batch : U64) (old : Option T) (change : Change T) :
    (do let (next, value) ← replay lsn batch old change
        replay next batch value change) = replay lsn batch old change := by
  unfold replay replay_needed
  cases change <;> by_cases h : lsn.val < batch.val <;> simp [apply, h]
/-- Sanity: a concrete no-op delta roundtrips to the same value. -/
theorem sanity_diff_apply :
    (do let change ← diff (some (1#u8)).isSome (some (1#u8)) true
        apply (some (1#u8)) change) ⦃ r => r = some (1#u8) ⦄ :=
  diff_apply (some 1#u8) (some 1#u8) true (by intro a b ha hb; simp_all)

/-- Sanity: a concrete older page image requests replay. -/
theorem sanity_replay :
    replay_needed 3#u64 5#u64 ⦃ r => r = true ⦄ := by
  apply WP.spec_mono (replay_correct 3#u64 5#u64)
  intro r hr
  rw [show (3#u64 : U64).val = 3 from rfl, show (5#u64 : U64).val = 5 from rfl] at hr
  cases r with
  | true => rfl
  | false => exact absurd (hr.mpr (by omega)) (by decide)

end delta_value_proof
