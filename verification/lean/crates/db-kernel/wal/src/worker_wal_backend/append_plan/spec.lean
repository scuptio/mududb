import crates.«db-kernel».wal.src.worker_wal_backend.append_plan.generated
open Aeneas Std Result
namespace append_plan_proof
open append_plan.append_plan

/-- Only the current chunk may contain previously reserved bytes. Every older
sequence is below `next`; a retired cursor has no remaining byte offset. -/
def Valid (s : AppendState) (capacity : U64) : Prop :=
  s.size.val ≤ capacity.val ∧
  match s.current with
  | none => s.size.val = 0
  | some seq => seq.val < s.next.val

/-- Exact placement contract, including zero-length identity and fresh-chunk
retirement. A fresh reservation consumes one sequence even for oversized data. -/
def Reserved (s : AppendState) (capacity length : U64) (p : AppendPlan) : Prop :=
  Valid p.state capacity ∧
  (p.flush = true ↔ capacity.val < length.val) ∧
  (length.val = 0 → p.state = s ∧ p.offset = s.size) ∧
  (length.val > 0 →
    ((s.current = some p.sequence ∧ p.offset = s.size ∧
      p.state.next = s.next ∧ p.offset.val + length.val ≤ capacity.val) ∨
     (p.sequence = s.next ∧ p.offset.val = 0 ∧ p.state.next.val = s.next.val + 1)) ∧
    (length.val ≤ capacity.val → p.offset.val + length.val ≤ capacity.val) ∧
    (if p.offset.val + length.val ≥ capacity.val
     then p.state.current = none ∧ p.state.size.val = 0
     else p.state.current = some p.sequence ∧ p.state.size.val = p.offset.val + length.val))
end append_plan_proof
