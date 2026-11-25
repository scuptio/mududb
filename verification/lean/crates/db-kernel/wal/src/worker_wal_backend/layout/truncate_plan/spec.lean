import crates.«db-kernel».wal.src.worker_wal_backend.layout.truncate_plan.generated
open Aeneas Aeneas.Std Result

namespace truncate_plan_proof
/-- Composition of the production cut predicate over validated frames. Each
pair contains the frame LSN and its complete encoded byte length. -/
def retainedBytes (boundary : U64) : List (U64 × Nat) → Result Nat
  | [] => ok 0
  | (lsn, size) :: rest => do
    let cut ← truncate_plan.truncate_plan.cut_before lsn boundary
    if cut then ok 0 else
      let tail ← retainedBytes boundary rest
      ok (size + tail)
end truncate_plan_proof
