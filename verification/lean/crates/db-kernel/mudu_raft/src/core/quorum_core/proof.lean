import crates.«db-kernel».mudu_raft.src.core.quorum_core.spec
open Aeneas Std Result
open raft_quorum_spec

namespace raft_quorum_proof

/-- One more position adds exactly the grant indicator at that position. -/
theorem count_prefix_succ (voters : List U64) (granted : List Bool) (exclude : U64) (i : Nat)
    (hi : i < voters.length) (hig : i < granted.length) :
    count_prefix voters granted exclude (i + 1) =
      count_prefix voters granted exclude i +
        (if voters[i]! != exclude && granted[i]! then 1 else 0) := by
  have hzl : i < (voters.zip granted).length := by simp only [List.length_zip]; omega
  have hz : (voters.zip granted)[i]? = some (voters[i]!, granted[i]!) := by
    rw [getElem?_pos (voters.zip granted) i hzl, List.getElem_zip (h := hzl)]
    show some (voters[i]'hi, granted[i]'hig) = some (voters[i]!, granted[i]!)
    rw [← getElem!_pos voters i hi, ← getElem!_pos granted i hig]
  simp only [count_prefix, List.take_add_one, List.filter_append, List.length_append]
  rw [hz, Option.toList_some]
  simp only [List.filter_cons, List.filter_nil]
  split <;> simp

/-- The agreeing count never exceeds the voter count. -/
theorem count_spec_le (voters : List U64) (granted : List Bool) (exclude : U64) :
    count_spec voters granted exclude ≤ voters.length := by
  simp only [count_spec, count_prefix]
  calc (((voters.zip granted).take voters.length).filter _).length
      ≤ ((voters.zip granted).take voters.length).length := List.length_filter_le _ _
    _ ≤ voters.length := by rw [List.length_take]; omega

/-- Unrolling one voter/grant pair through `count_spec`. -/
theorem count_spec_cons (v : U64) (vs : List U64) (g : Bool) (gs : List Bool) (e : U64) :
    count_spec (v :: vs) (g :: gs) e =
      (if v != e && g then 1 else 0) + count_spec vs gs e := by
  simp only [count_spec, count_prefix, List.length_cons, List.zip_cons_cons,
    List.take_succ_cons, List.filter_cons]
  by_cases hc : (v != e && g) <;> simp [hc]; omega

/-- Unrolling one voter/grant pair through `count_both`. -/
theorem count_both_cons (v : U64) (vs : List U64) (g1 : Bool) (gs1 : List Bool)
    (g2 : Bool) (gs2 : List Bool) (e : U64) :
    count_both (v :: vs) (g1 :: gs1) (g2 :: gs2) e =
      (if v != e && g1 && g2 then 1 else 0) + count_both vs gs1 gs2 e := by
  simp only [count_both, List.zip_cons_cons, List.filter_cons]
  by_cases hc : (v != e && g1 && g2) <;> simp [hc]; omega

/-- The pigeonhole bound behind quorum intersection: two position-aligned
    counts over the same voter list cannot both exceed half without
    overlapping. -/
theorem count_add_le (voters : List U64) (ga gb : List Bool) (e : U64) :
    count_spec voters ga e + count_spec voters gb e ≤
      voters.length + count_both voters ga gb e := by
  induction voters generalizing ga gb with
  | nil => simp [count_spec, count_prefix, count_both]
  | cons v vs ih =>
    cases ga with
    | nil =>
      have hz : count_spec (v :: vs) ([] : List Bool) e = 0 := by
        simp [count_spec, count_prefix]
      have hb : count_both (v :: vs) ([] : List Bool) gb e = 0 := by simp [count_both]
      rw [hz, hb, List.length_cons]
      have hle := count_spec_le (v :: vs) gb e
      rw [List.length_cons] at hle
      omega
    | cons g1 gs1 =>
      cases gb with
      | nil =>
        have hz : count_spec (v :: vs) ([] : List Bool) e = 0 := by
          simp [count_spec, count_prefix]
        have hb : count_both (v :: vs) (g1 :: gs1) ([] : List Bool) e = 0 := by simp [count_both]
        rw [hz, hb, List.length_cons]
        have hle := count_spec_le (v :: vs) (g1 :: gs1) e
        rw [List.length_cons] at hle
        omega
      | cons g2 gs2 =>
        have h := ih gs1 gs2
        rw [count_spec_cons, count_spec_cons, count_both_cons, List.length_cons]
        by_cases hve : (v != e) = true
        · cases g1 <;> cases g2 <;> simp [hve] <;> omega
        · have hvef : (v != e) = false := by
            cases hev : (v != e)
            · rfl
            · exact absurd hev hve
          simp [hvef]; omega

/-- Two strict majorities over the same voter list intersect: some
    non-excluded voter granted both. -/
theorem quorum_intersection (voters : List U64) (ga gb : List Bool) (exclude : U64)
    (ha : majority (count_spec voters ga exclude) voters.length)
    (hb : majority (count_spec voters gb exclude) voters.length) :
    ∃ v g1 g2, (v, (g1, g2)) ∈ voters.zip (ga.zip gb) ∧ v ≠ exclude ∧ g1 = true ∧ g2 = true := by
  have h := count_add_le voters ga gb exclude
  have hpos : 0 < count_both voters ga gb exclude := by simp only [majority] at ha hb; omega
  have hne : (voters.zip (ga.zip gb)).filter (fun p => p.1 != exclude && p.2.1 && p.2.2) ≠ [] :=
    List.ne_nil_of_length_pos (by simpa only [count_both] using hpos)
  obtain ⟨⟨v, g1, g2⟩, hx⟩ := List.exists_mem_of_ne_nil _ hne
  rw [List.mem_filter] at hx
  obtain ⟨hxmem, hpred⟩ := hx
  simp only [Bool.and_eq_true, bne_iff_ne] at hpred
  exact ⟨v, g1, g2, hxmem, hpred.1.1, hpred.1.2, hpred.2⟩

set_option maxHeartbeats 1000000 in
/-- The extracted counting loop computes exactly the specification count. -/
theorem count_correct (voters : Aeneas.Std.Slice U64) (granted : Aeneas.Std.Slice Bool)
    (exclude : U64) (hlen : granted.val.length = voters.val.length) :
    quorum_core.quorum_core.count_agreeing_excluding voters granted exclude
      ⦃ r => r.val = count_spec voters.val granted.val exclude ⦄ := by
  have hbnd := Slice.length_ineq voters
  unfold quorum_core.quorum_core.count_agreeing_excluding
    quorum_core.quorum_core.count_agreeing_excluding_loop
  apply loop.spec_decr_nat
    (measure := fun (x : Usize × Usize) => voters.val.length - x.2.val)
    (inv := fun (x : Usize × Usize) => x.2.val ≤ voters.val.length ∧ x.1.val ≤ x.2.val ∧
        x.1.val = count_prefix voters.val granted.val exclude x.2.val)
    (post := fun (r : Usize) => r.val = count_spec voters.val granted.val exclude)
  · intro x hinv
    obtain ⟨n, i⟩ := x
    obtain ⟨hile, hnle, hn⟩ := hinv
    dsimp only
    unfold quorum_core.quorum_core.count_agreeing_excluding_loop.body
    dsimp only
    split
    · rename_i hlt
      step as ⟨i2, hi2⟩
      have e2 : i2 = voters.val[i.val]! := by
        simpa only [getElem!_pos voters.val i.val (by scalar_tac)] using hi2
      have hn' : n.val = count_prefix voters.val granted.val exclude i.val := hn
      have hnle' : n.val ≤ i.val := hnle
      have hi' : i.val < voters.val.length := by scalar_tac
      have hig' : i.val < granted.val.length := by scalar_tac
      split
      · rename_i hne
        step as ⟨b, hb⟩
        have eb : b = granted.val[i.val]! := by
          simpa only [getElem!_pos granted.val i.val (by scalar_tac)] using hb
        split
        · rename_i hbt
          step as ⟨n1, hn1⟩
          step as ⟨i3, hi3⟩
          refine ⟨by omega, by omega, ?_, by omega⟩
          rw [hi3, count_prefix_succ voters.val granted.val exclude i.val hi' hig',
            ← hn', hn1, ← e2, ← eb]
          simp [hne, hbt]
        · rename_i hbt
          have hbf : b = false := by
            cases b
            · rfl
            · exact absurd rfl hbt
          step as ⟨n1, hn1⟩
          refine ⟨by omega, by omega, ?_, by omega⟩
          rw [hn1, count_prefix_succ voters.val granted.val exclude i.val hi' hig',
            ← hn', ← e2, ← eb]
          simp [hne, hbf]
      · rename_i hne
        have hnef : (i2 != exclude) = false := by
          cases he : (i2 != exclude)
          · rfl
          · exact absurd he hne
        step as ⟨n1, hn1⟩
        refine ⟨by omega, by omega, ?_, by omega⟩
        rw [hn1, count_prefix_succ voters.val granted.val exclude i.val hi' hig',
          ← hn', ← e2]
        simp [hnef]
    · rename_i hge
      rw [WP.spec_ok]
      show n.val = count_spec voters.val granted.val exclude
      have hn' : n.val = count_prefix voters.val granted.val exclude i.val := hn
      have hile' : i.val ≤ voters.length := hile
      have hiv : i.val = voters.length := by scalar_tac
      rw [hn', hiv]
      rfl
  · refine ⟨by scalar_tac, by scalar_tac, ?_⟩
    show (0#usize).val = count_prefix voters.val granted.val exclude (0#usize).val
    have h0 : (0#usize : Usize).val = 0 := by scalar_tac
    rw [h0]
    rfl

/-- The count never exceeds the voter count. -/
theorem count_bounded (voters : Aeneas.Std.Slice U64) (granted : Aeneas.Std.Slice Bool)
    (exclude : U64) (hlen : granted.val.length = voters.val.length) :
    quorum_core.quorum_core.count_agreeing_excluding voters granted exclude
      ⦃ r => r.val ≤ voters.length ⦄ := by
  have h := count_correct voters granted exclude hlen
  apply WP.spec_mono h
  intro r hr
  rw [hr]
  exact count_spec_le voters.val granted.val exclude

/-- The extracted majority check is strict majority on the counts. -/
theorem majority_met_iff (agreed voters : Usize) (h : agreed.val * 2 ≤ Usize.max) :
    quorum_core.quorum_core.majority_met agreed voters ⦃ r =>
      (r = true) ↔ majority agreed.val voters.val ⦄ := by
  unfold quorum_core.quorum_core.majority_met
  step as ⟨i, hi⟩
  have hi' : i.val = agreed.val * 2 := by scalar_tac
  simp [majority, decide_eq_true_eq, hi']

/-- The majority pick of a complete, non-empty descending match list is the
    `(nodes / 2)`-th element (the high median). -/
theorem pick_value_correct (sorted_desc : Aeneas.Std.Slice U64) (nodes : Usize)
    (hlen : sorted_desc.val.length = nodes.val) (hpos : 0 < nodes.val) :
    quorum_core.quorum_core.majority_pick sorted_desc nodes ⦃ r =>
      r = some sorted_desc.val[nodes.val / 2]! ⦄ := by
  unfold quorum_core.quorum_core.majority_pick
  dsimp only
  split
  · split
    · step as ⟨i, hi⟩
      have hi' : i.val = nodes.val / 2 := by scalar_tac
      step as ⟨i1, hi1⟩
      have e1 : i1 = sorted_desc.val[i.val]! := by
        simpa only [getElem!_pos sorted_desc.val i.val (by scalar_tac)] using hi1
      rw [e1, hi']
    · exfalso
      scalar_tac
  · rename_i hne
    exfalso
    apply hne
    apply UScalar.eq_of_val_eq
    scalar_tac

/-- The pick is `none` when the list is not exactly `nodes` long. -/
theorem pick_none_incomplete (sorted_desc : Aeneas.Std.Slice U64) (nodes : Usize)
    (hlen : sorted_desc.val.length ≠ nodes.val) :
    quorum_core.quorum_core.majority_pick sorted_desc nodes ⦃ r => r = none ⦄ := by
  unfold quorum_core.quorum_core.majority_pick
  dsimp only
  split
  · rename_i heq
    exfalso
    apply hlen
    have h1 := congrArg UScalar.val heq
    simp only [Slice.len_val] at h1
    exact h1
  · rw [WP.spec_ok]

/-- Canary: a 3-node vote with one follower grant plus the leader reaches a
    majority. -/
theorem sanity_vote_quorum :
    (do
      let n ← quorum_core.quorum_core.count_agreeing_excluding
        (Slice.from [1#u64, 2#u64, 3#u64] (by scalar_tac))
        (Slice.from [false, true, false] (by scalar_tac)) 1#u64
      let m ← n + 1#usize
      quorum_core.quorum_core.majority_met m 3#usize) ⦃ r => r = true ⦄ := by
  have hlen :
      (Slice.from [false, true, false] (by scalar_tac) : Aeneas.Std.Slice Bool).val.length =
        (Slice.from [1#u64, 2#u64, 3#u64] (by scalar_tac) : Aeneas.Std.Slice U64).val.length := by
    rw [Aeneas.Std.Slice.from_val, Aeneas.Std.Slice.from_val]
    rfl
  step with count_correct (hlen := hlen) as ⟨n, hn⟩
  have hn1 : n.val = 1 := by
    rw [hn]
    simp only [Aeneas.Std.Slice.from_val]
    decide
  step as ⟨m, hm⟩
  have hm2 : m = 2#usize := UScalar.eq_of_val_eq (by scalar_tac)
  rw [hm2]
  have h := majority_met_iff 2#usize 3#usize (by scalar_tac)
  apply WP.spec_mono h
  intro r hr
  exact hr.mpr (by simp only [majority]; scalar_tac)

/-- Canary: the majority pick of a 3-node descending match list is the high
    median. -/
theorem sanity_match_index :
    quorum_core.quorum_core.majority_pick
      (Slice.from [10#u64, 7#u64, 4#u64] (by scalar_tac)) 3#usize ⦃ r => r = some 7#u64 ⦄ := by
  have h := pick_value_correct (Slice.from [10#u64, 7#u64, 4#u64] (by scalar_tac)) 3#usize
    (by rw [Aeneas.Std.Slice.from_val]; scalar_tac) (by scalar_tac)
  apply WP.spec_mono h
  intro r hr
  rw [hr]
  simp only [Aeneas.Std.Slice.from_val]
  have hv : (3#usize : Usize).val = 3 := by scalar_tac
  rw [hv]
  decide

end raft_quorum_proof
