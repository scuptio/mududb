import crates.«db-kernel».wal.src.worker_wal_backend.layout.decision.spec
open Aeneas Std Result
namespace wal_decision_proof

/-- Exhaustion is explicit: a successful successor is strictly greater. -/
@[step] theorem successor_correct (v : U64) :
    decision.decision.successor v ⦃ r =>
      match r with
      | none => v.val = 18446744073709551615
      | some n => n.val = v.val + 1 ⦄ := by
  unfold decision.decision.successor
  have hm : core.num.U64.MAX = 18446744073709551615#u64 := by rfl
  rw [hm]
  split
  · simp_all
  · step
    simp_all

/-- Recovery never allocates below the persisted reclamation frontier. -/
theorem recovery_correct (frontier : U64) (maximum : Option U64) :
    decision.decision.recovery_lsn frontier maximum ⦃ r =>
      match r with
      | none => maximum = some (UScalar.ofNat 18446744073709551615)
      | some n => frontier.val ≤ n.val ∧
          (∀ v, maximum = some v → n.val = max frontier.val (v.val + 1)) ⦄ := by
  unfold decision.decision.recovery_lsn
  cases maximum with
  | none => simp
  | some v =>
    step as ⟨r, hr⟩
    cases r with
    | none => simp_all; scalar_tac
    | some n =>
      simp only at *
      split <;> simp_all; scalar_tac

/-- A tail is reused exactly when its validated prefix fits below capacity. -/
theorem reuse_correct (size capacity : U64) :
    decision.decision.reuse_tail size capacity ⦃ r => r = true ↔ size.val < capacity.val ⦄ := by
  simp [decision.decision.reuse_tail]

/-- GC never moves backwards, never exceeds the request/old-frontier maximum,
and never exceeds an enabled archive frontier. -/
theorem gc_limit_correct (requested existing : U64) (archive : Option U64) :
    decision.decision.gc_limit requested existing archive ⦃ r =>
      match r with
      | none => ∃ a, archive = some a ∧ a.val < existing.val
      | some limit => existing.val ≤ limit.val ∧
          limit.val ≤ max requested.val existing.val ∧
          (∀ a, archive = some a → limit.val ≤ a.val) ⦄ := by
  unfold decision.decision.gc_limit
  split
  all_goals cases archive <;> simp_all
  all_goals split <;> simp_all
  all_goals try split <;> simp_all; scalar_tac

/-- Strict deletion excludes the append tail, unknown chunks, and equality. -/
theorem deletion_correct (tail : Bool) (maximum : Option U64) (limit : U64) :
    decision.decision.deletable tail maximum limit ⦃ r =>
      r = true ↔ tail = false ∧ ∃ m, maximum = some m ∧ m.val < limit.val ⦄ := by
  cases tail <;> cases maximum <;> simp [decision.decision.deletable]
/-- Sanity: the recovery contract succeeds on concrete frontier inputs. -/
theorem sanity_recovery :
    decision.decision.recovery_lsn 42#u64 (some 41#u64) ⦃ r =>
      ∃ n, r = some n ∧ n.val = 42 ⦄ := by
  have h := recovery_correct 42#u64 (some 41#u64)
  apply WP.spec_mono h
  intro r hr
  cases r with
  | none => exact absurd hr (by decide)
  | some n =>
    have hv := hr.2 41#u64 rfl
    rw [show (42#u64 : U64).val = 42 from rfl, show (41#u64 : U64).val = 41 from rfl] at hv
    exact ⟨n, rfl, by omega⟩

/-- Sanity: a concrete GC limit succeeds and respects the archive frontier. -/
theorem sanity_gc :
    decision.decision.gc_limit 100#u64 42#u64 (some 60#u64) ⦃ r =>
      ∃ limit, r = some limit ∧ 42 ≤ limit.val ∧ limit.val ≤ 60 ⦄ := by
  have h := gc_limit_correct 100#u64 42#u64 (some 60#u64)
  apply WP.spec_mono h
  intro r hr
  cases r with
  | none =>
    obtain ⟨a, ha, hlt⟩ := hr
    have ha' : a = 60#u64 := by simpa using ha.symm
    rw [ha', show (60#u64 : U64).val = 60 from rfl, show (42#u64 : U64).val = 42 from rfl] at hlt
    omega
  | some limit =>
    obtain ⟨h1, -, h3⟩ := hr
    rw [show (42#u64 : U64).val = 42 from rfl] at h1
    have h2 := h3 60#u64 rfl
    rw [show (60#u64 : U64).val = 60 from rfl] at h2
    exact ⟨limit, rfl, h1, h2⟩

end wal_decision_proof
