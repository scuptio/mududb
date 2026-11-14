import crates.«db-kernel».mudu_raft.src.core.quorum_core.generated
open Aeneas Std Result

namespace raft_quorum_spec

/-- The agreeing count over the first `i` position-aligned voters: positions
    whose voter id differs from `exclude` and whose grant flag is set. -/
def count_prefix (voters : List U64) (granted : List Bool) (exclude : U64) (i : Nat) : Nat :=
  (((voters.zip granted).take i).filter (fun p => p.1 != exclude && p.2)).length

/-- The full agreeing count over position-aligned voter/grant lists. -/
def count_spec (voters : List U64) (granted : List Bool) (exclude : U64) : Nat :=
  count_prefix voters granted exclude voters.length

/-- The count of positions where both grant flags are set and the voter id
    differs from `exclude`. -/
def count_both (voters : List U64) (ga gb : List Bool) (exclude : U64) : Nat :=
  ((voters.zip (ga.zip gb)).filter (fun p => p.1 != exclude && p.2.1 && p.2.2)).length

/-- Strict majority: more than half of the voters agreed. -/
def majority (agreed voters : Nat) : Prop :=
  agreed * 2 > voters

end raft_quorum_spec
