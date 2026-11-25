import crates.«db-kernel».wal.src.frame_bounds.generated
open Aeneas Std Result
namespace frame_bounds_proof
open frame_bounds.frame_bounds

/-- Acceptance depends on validation and available bytes, never on bytes after
this candidate. CRC correctness itself is outside this arithmetic contract. -/
abbrev Accept (p : Prefix) (total length : Usize) (valid : Bool) : Prop :=
  p.stopped = false ∧ valid = true ∧ 0 < length.val ∧ p.offset.val + length.val ≤ total.val

/-- Composition of the extracted production cursor over validation outcomes.
The filesystem/parser supplies these outcomes; no replacement Rust is modeled. -/
def scan (p : Prefix) (total : Usize) : List (Usize × Bool) → Result Prefix
  | [] => ok p
  | (length, valid) :: rest => do
    let q ← Prefix.advance p total length valid
    scan q total rest

/-- Mathematical lengths of the maximal accepted prefix. Once a candidate is
invalid, empty or out of bounds, no later length contributes to the result. -/
def acceptedLengths (offset total : Nat) (stopped : Bool) : List (Usize × Bool) → List Nat
  | [] => []
  | (length, valid) :: rest =>
    if stopped = false ∧ valid = true ∧ 0 < length.val ∧ offset + length.val ≤ total
    then length.val :: acceptedLengths (offset + length.val) total false rest
    else []
end frame_bounds_proof
